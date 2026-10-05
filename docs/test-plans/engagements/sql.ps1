# SPDX-License-Identifier: Apache-2.0
# These commands run only after engagement prerequisites pass. They never DROP,
# REPLACE or overwrite an existing database; the original database is read only.
function Invoke-EngagementSql([string]$Label, [string]$Database, [string]$Query) {
    if ($Database -notmatch '^[A-Za-z0-9_-]{1,128}$') { throw 'Unsafe database identifier.' }
    $sqlFile = Join-Path $ctx.Logs ("sql-$Label-$([guid]::NewGuid().ToString('N')).sql")
    Write-Utf8File $sqlFile ("SET NOCOUNT ON; SET XACT_ABORT ON;`n" + $Query)
    $run = Invoke-Tool -Ctx $ctx -Stage 'SQL' -Label $Label -FilePath $sqlcmd -ArgumentList @(
        '-S', $sqlServer, '-d', $Database, '-E', '-b', '-r', '1', '-f', '65001', '-h', '-1', '-y', '0', '-w', '65535', '-i', $sqlFile)
    Assert-That ($run.ExitCode -eq 0) "SQL failed ($Label): $($run.Errors)"
    return $run.Output.Trim()
}
function Get-EngagementSqlJson([string]$Label, [string]$Database, [string]$Query) {
    $text = Invoke-EngagementSql $Label $Database $Query
    # sqlcmd can split its single FOR JSON scalar over physical output lines.
    return (($text -split "`r?`n") -join '') | ConvertFrom-Json -AsHashtable
}
function Get-DatabaseEvidence([string]$Database, [string]$Label) {
    $tables = @(Get-EngagementSqlJson "$Label-tables" $Database "SELECT SCHEMA_NAME(schema_id) AS [schema], name FROM sys.tables ORDER BY schema_id,name FOR JSON PATH;")
    $rows = [ordered]@{}
    foreach ($table in $tables) {
        $qualified = '[' + $table.schema.Replace(']', ']]') + '].[' + $table.name.Replace(']', ']]') + ']'
        $rows["$($table.schema).$($table.name)"] = @(Get-EngagementSqlJson "$Label-rows" $Database "SELECT * FROM $qualified FOR JSON PATH, INCLUDE_NULL_VALUES;")
    }
    $rows['__schema'] = @(Get-EngagementSqlJson "$Label-schema" $Database @'
SELECT o.object_id,o.schema_id,o.name,o.type,o.create_date,o.modify_date,
JSON_QUERY((SELECT c.column_id,c.name,c.system_type_id,c.user_type_id,c.max_length,c.precision,c.scale,c.is_nullable,c.is_identity,c.is_computed,c.collation_name FROM sys.columns c WHERE c.object_id=o.object_id ORDER BY c.column_id FOR JSON PATH,INCLUDE_NULL_VALUES)) AS columns,
JSON_QUERY((SELECT i.index_id,i.name,i.type,i.is_unique,i.is_primary_key,i.is_unique_constraint,i.is_disabled,i.has_filter,i.filter_definition,
JSON_QUERY((SELECT x.column_id,x.key_ordinal,x.is_descending_key,x.is_included_column FROM sys.index_columns x WHERE x.object_id=i.object_id AND x.index_id=i.index_id ORDER BY x.index_column_id FOR JSON PATH)) AS columns
FROM sys.indexes i WHERE i.object_id=o.object_id ORDER BY i.index_id FOR JSON PATH,INCLUDE_NULL_VALUES)) AS indexes,
OBJECT_DEFINITION(o.object_id) AS definition,
JSON_QUERY((SELECT f.parent_object_id,f.parent_column_id,f.referenced_object_id,f.referenced_column_id FROM sys.foreign_key_columns f WHERE f.constraint_object_id=o.object_id ORDER BY f.constraint_column_id FOR JSON PATH)) AS foreignKeys
FROM sys.objects o WHERE o.is_ms_shipped=0 ORDER BY o.object_id FOR JSON PATH,INCLUDE_NULL_VALUES;
'@)
    return $rows
}
function Initialize-EngagementDatabase {
    $settings = Get-Content -LiteralPath (Join-Path $ws 'src/Inventory.Web/appsettings.Development.json') -Raw | ConvertFrom-Json
    $builder = [System.Data.Common.DbConnectionStringBuilder]::new(); $builder.set_ConnectionString($settings.ConnectionStrings.Inventory)
    $sourceServer = if ($builder.ContainsKey('Server')) { [string]$builder['Server'] } else { [string]$builder['Data Source'] }
    $sourceDatabase = if ($builder.ContainsKey('Database')) { [string]$builder['Database'] } else { [string]$builder['Initial Catalog'] }
    Assert-That ($sourceServer -eq $spec.B.database.server -and $sourceDatabase -eq $spec.B.database.name) 'Declared baseline database must match protected original configuration.'
    Assert-That ($sourceServer -match '^(\(localdb\)\\[A-Za-z0-9_-]+|localhost(?:\\[A-Za-z0-9_-]+)?|\.(?:\\[A-Za-z0-9_-]+)?)$') 'Only a local integrated-auth SQL Server can be cloned by this runner.'
    $script:sqlServer = $sourceServer
    $script:database = 'VcpEngagement_' + [guid]::NewGuid().ToString('N')
    $script:freshDatabase = 'VcpEngagementFresh_' + [guid]::NewGuid().ToString('N')
    # Existing helper validates integrated auth and rejects passwords/attach paths.
    $script:connection = New-InventoryConnection $settings.ConnectionStrings.Inventory $database
    $script:freshConnection = New-InventoryConnection $settings.ConnectionStrings.Inventory $freshDatabase
    $script:useLocalDb = $sqlServer -like '(localdb)*'
    $before = Get-DatabaseEvidence $sourceDatabase 'original-before-copy'
    Write-JsonFile (Join-Path $ctx.Results 'original-database-before.json') $before
    $backup = Join-Path $ctx.Temp 'baseline-copy.bak'
    Assert-That (-not (Test-Path -LiteralPath $backup)) 'Backup target must be new.'
    $files = @(Get-EngagementSqlJson 'source-files' 'master' "SELECT name,type FROM sys.master_files WHERE database_id=DB_ID(N'$sourceDatabase') ORDER BY file_id FOR JSON PATH;")
    Assert-That ($files.Count -ge 2) 'Missing source database file inventory.'
    [void](Invoke-EngagementSql 'backup-copy-only' 'master' "BACKUP DATABASE [$sourceDatabase] TO DISK=N'$($backup.Replace("'", "''"))' WITH COPY_ONLY, CHECKSUM;")
    $moves = for ($n = 0; $n -lt $files.Count; $n++) {
        $destination = Join-Path $ctx.Temp ("$database-$n" + $(if ($files[$n].type -eq 1) { '.ldf' } else { '.mdf' }))
        Assert-That (-not (Test-Path -LiteralPath $destination)) 'Database file target must be new.'
        "MOVE N'$($files[$n].name.Replace("'", "''"))' TO N'$($destination.Replace("'", "''"))'"
    }
    [void](Invoke-EngagementSql 'restore-isolated-copy' 'master' "IF DB_ID(N'$database') IS NOT NULL THROW 50001,'Destination exists',1; RESTORE DATABASE [$database] FROM DISK=N'$($backup.Replace("'", "''"))' WITH CHECKSUM,$($moves -join ',');")
    [void](Invoke-EngagementSql 'create-fresh' 'master' "IF DB_ID(N'$freshDatabase') IS NOT NULL THROW 50001,'Destination exists',1; CREATE DATABASE [$freshDatabase];")
    Write-JsonFile (Join-Path $ctx.Results 'database-identities.json') ([ordered]@{ server=$sqlServer; original=$sourceDatabase; copy=$database; fresh=$freshDatabase; backup=$backup; cleanup='retained; no automatic deletion' })
    $script:originalDatabase = $sourceDatabase
    $script:originalDatabaseRows = $before
    $script:copyRows = Get-DatabaseEvidence $database 'copied-before-migration'
    Write-JsonFile (Join-Path $ctx.Results 'copied-database-before.json') $copyRows
}
function Test-DatabasePreserved([hashtable]$Before, [hashtable]$After, [switch]$AllowMigrationAppend) {
    if (-not $AllowMigrationAppend) {
        Assert-That ((@($Before.Keys | Sort-Object) -join "`n") -ceq (@($After.Keys | Sort-Object) -join "`n")) 'Original database table set changed.'
    }
    foreach ($table in $Before.Keys) {
        if ($AllowMigrationAppend -and $table -eq '__schema') { continue }
        Assert-That ($After.Contains($table) -or $After.ContainsKey($table)) "Missing original table $table"
        $left = @($Before[$table] | ForEach-Object { $_ | ConvertTo-Json -Depth 32 -Compress } | Sort-Object)
        $right = @($After[$table] | ForEach-Object { $_ | ConvertTo-Json -Depth 32 -Compress } | Sort-Object)
        if ($AllowMigrationAppend -and $table -eq 'dbo.__EFMigrationsHistory') {
            foreach ($row in $left) { Assert-That ($right -ccontains $row) 'Existing migration identity changed.' }
        } else { Assert-That (($left -join "`n") -ceq ($right -join "`n")) "Existing rows changed in $table" }
    }
}
function Test-AdjustmentSql([string]$StatePath, [string]$Label) {
    $state = Get-Content -LiteralPath $StatePath -Raw | ConvertFrom-Json
    $productId = [int]$state.productId
    $query = @"
SELECT CAST(CASE WHEN EXISTS (
 SELECT 1 FROM sys.indexes i WHERE i.object_id=OBJECT_ID(N'dbo.StockAdjustments') AND i.is_unique=1 AND i.has_filter=0 AND i.is_disabled=0 AND i.is_hypothetical=0
 AND (SELECT COUNT(*) FROM sys.index_columns c WHERE c.object_id=i.object_id AND c.index_id=i.index_id AND c.key_ordinal>0)=1
 AND EXISTS (SELECT 1 FROM sys.index_columns c JOIN sys.columns x ON x.object_id=c.object_id AND x.column_id=c.column_id
 WHERE c.object_id=i.object_id AND c.index_id=i.index_id AND c.key_ordinal=1 AND x.name=N'OperationId')
) THEN 1 ELSE 0 END AS bit) AS uniqueOperationId,
COALESCE((SELECT SUM(Quantity) FROM dbo.StockMovements WHERE ProductId=$productId),0) AS onHand,
JSON_QUERY((SELECT Id AS id,ProductId AS productId,Delta AS delta,BeforeQuantity AS beforeQuantity,
AfterQuantity AS afterQuantity,Reason AS reason,TODATETIMEOFFSET([Timestamp],'+00:00') AS timestamp,OperationId AS operationId
FROM dbo.StockAdjustments WHERE ProductId=$productId ORDER BY [Timestamp],Id FOR JSON PATH)) AS adjustments
FOR JSON PATH,WITHOUT_ARRAY_WRAPPER;
"@
    $observed = Get-EngagementSqlJson $Label $database $query
    $file = Join-Path $ctx.Results "$Label-sql.json"; Write-JsonFile $file $observed
    $check = Invoke-Tool -Ctx $ctx -Stage $Label -Label 'sql-api-agreement' -FilePath $node -ArgumentList @((Join-Path $PSScriptRoot 'sql-check.cjs'), $StatePath, $file)
    Assert-That ($check.ExitCode -eq 0) $check.Errors
}
