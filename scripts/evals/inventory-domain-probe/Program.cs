// SPDX-License-Identifier: Apache-2.0
// External T1 quality oracle. Never invokes the application entry point or generated tests.
using System.ComponentModel.DataAnnotations;
using System.Reflection;
using System.Runtime.Loader;
using System.Security.Cryptography;
using System.Text.Json;

return Probe.Run(args);

static class Probe
{
    public static int Run(string[] args)
    {
        try
        {
            var options = Options(args);
            var workspace = Physical(options["--workspace"], directory: true);
            var assembly = Physical(options["--assembly"], directory: false);
            var terminal = Physical(options["--terminal-jsonl"], directory: false);
            var output = Path.GetFullPath(options["--out"]);
            if (Inside(workspace, output) || File.Exists(output)) throw new InvalidOperationException("Report must be new and outside the generated workspace.");
            Physical(Path.GetDirectoryName(output)!, directory: true);
            if (!Inside(workspace, assembly)) throw new InvalidOperationException("Assembly must belong to the selected workspace.");
            var receipt = ReadTerminal(terminal);
            var before = Fingerprint(workspace, assembly);
            var context = new ModelContext(assembly);
            var model = context.LoadFromAssemblyPath(assembly).GetType("Inventory.Web.Data.StockMovement", throwOnError: false);
            var checks = new List<Check>();
            checks.Add(new("stock-movement-type", model is { IsClass: true, IsAbstract: false }));
            var quantity = model?.GetProperty("Quantity");
            var reason = model?.GetProperty("Reason");
            checks.Add(new("quantity-int", quantity?.PropertyType == typeof(int) && quantity.CanWrite));
            var enumType = reason?.PropertyType;
            var enumCorrect = enumType?.IsEnum == true && reason!.CanWrite
                && Enum.GetNames(enumType).Order().SequenceEqual(new[] { "Adjustment", "Receipt", "Sale" });
            checks.Add(new("reason-exact-enum", enumCorrect));
            // T1 explicitly requires this namespace, enum and zero rejection.
            // Exercise actual DataAnnotations/IValidatableObject validation, not a test name.
            if (model is not null && quantity?.PropertyType == typeof(int) && quantity.CanWrite && reason?.CanWrite == true)
            {
                foreach (var sample in new[] { (0, "Receipt", false), (0, "Sale", false), (0, "Adjustment", false), (1, "Receipt", true), (-1, "Receipt", true), (1, "Sale", true), (-1, "Sale", true), (1, "Adjustment", true), (-1, "Adjustment", true) })
                {
                    var instance = Activator.CreateInstance(model) ?? throw new InvalidOperationException("Cannot construct model.");
                    Set(instance, "Id", 1); Set(instance, "ProductId", 1);
                    quantity.SetValue(instance, sample.Item1);
                    if (enumCorrect) reason.SetValue(instance, Enum.Parse(enumType!, sample.Item2));
                    else if (reason.PropertyType == typeof(string)) reason.SetValue(instance, sample.Item2);
                    Set(instance, "OccurredAt", DateTime.UtcNow); Set(instance, "Note", "Independent domain probe");
                    var valid = Validator.TryValidateObject(instance, new ValidationContext(instance), new List<ValidationResult>(), validateAllProperties: true);
                    checks.Add(new($"quantity-{sample.Item1}-{sample.Item2}", valid == sample.Item3));
                }
            }
            else checks.Add(new("model-validation-executable", false));
            var after = Fingerprint(workspace, assembly);
            if (before != after || Hash(terminal) != receipt.Hash) throw new InvalidOperationException("Source, assembly dependencies or terminal evidence changed during probe.");
            var passed = checks.All(check => check.Passed);
            var report = new
            {
                schema = "vcp-inventory-domain-probe/1", passed, workspace,
                source_and_binary_sha256 = before, assembly = new { path = assembly, sha256 = Hash(assembly) },
                terminal = new { path = terminal, sha256 = receipt.Hash, task = receipt.Task },
                evaluator_sha256 = Hash(Assembly.GetExecutingAssembly().Location), checks,
                coverage = new[] { "StockMovement namespace/type", "integer Quantity", "exact Reason enum members", "zero invalid and positive/negative contract examples through actual model validation" },
                limitations = new[] { "Caller must keep the completed owner and other workspace writers stopped; terminal evidence and before/after fingerprints detect changes, not future resumption.", "Source fingerprint is diagnostic only, not proof this assembly was built from that source. The owner-controlled harness must complete a fresh successful build immediately before this probe at the same quiescent stage.", "Does not qualify SQL enum conversion, max lengths, migrations, UTC persistence, other entities, API or complete Scenario B acceptance." }
            };
            using var stream = new FileStream(output, FileMode.CreateNew, FileAccess.Write, FileShare.Read);
            JsonSerializer.Serialize(stream, report, new JsonSerializerOptions { WriteIndented = true });
            Console.WriteLine(passed ? "Domain probe passed." : "Domain probe failed; inspect bounded check results.");
            return passed ? 0 : 1;
        }
        catch (Exception error)
        {
            // Application exception messages may contain arbitrary payload; retain only category.
            Console.Error.WriteLine($"Domain probe refused or could not complete ({error.GetType().Name}).");
            return 2;
        }
    }

    static Dictionary<string, string> Options(string[] args)
    {
        var result = new Dictionary<string, string>();
        if (args.Length != 8) throw new ArgumentException("Expected four named options.");
        for (var i = 0; i < args.Length; i += 2)
            if (!new[] { "--workspace", "--assembly", "--terminal-jsonl", "--out" }.Contains(args[i]) || !result.TryAdd(args[i], args[i + 1])) throw new ArgumentException("Invalid option.");
        return result;
    }
    static bool Inside(string root, string child) => Path.GetFullPath(child).StartsWith(Path.TrimEndingDirectorySeparator(root) + Path.DirectorySeparatorChar, OperatingSystem.IsWindows() ? StringComparison.OrdinalIgnoreCase : StringComparison.Ordinal);
    static string Physical(string path, bool directory)
    {
        path = Path.GetFullPath(path);
        if (directory ? !Directory.Exists(path) : !File.Exists(path)) throw new IOException("Missing input.");
        for (var item = path; item is not null; item = Path.GetDirectoryName(item))
            if ((File.GetAttributes(item) & FileAttributes.ReparsePoint) != 0) throw new IOException("Linked input denied.");
        return path;
    }
    static string Hash(string path)
    {
        using var stream = File.OpenRead(path);
        return Convert.ToHexString(SHA256.HashData(stream)).ToLowerInvariant();
    }
    static (string Hash, string Task) ReadTerminal(string path)
    {
        if (new FileInfo(path).Length > 64 * 1024 * 1024) throw new IOException("Terminal evidence exceeds bound.");
        var rows = File.ReadLines(path).Where(line => !string.IsNullOrWhiteSpace(line)).Select(line => JsonDocument.Parse(line)).ToList();
        try
        {
            var accepted = rows.Where(row => row.RootElement.GetProperty("type").GetString() == "accepted").ToList();
            var results = rows.Where(row => row.RootElement.GetProperty("type").GetString() == "result").ToList();
            if (accepted.Count != 1 || results.Count != 1 || !ReferenceEquals(rows.Last(), results[0])) throw new InvalidOperationException("No unique terminal result.");
            var first = accepted[0].RootElement; var last = results[0].RootElement;
            if (last.GetProperty("exit_code").GetInt32() != 0 || string.IsNullOrEmpty(first.GetProperty("correlation").GetString()) || first.GetProperty("correlation").GetString() != last.GetProperty("correlation").GetString()) throw new InvalidOperationException("Owner did not complete successfully.");
            foreach (var key in new[] { "workspace", "session", "task" })
            {
                var value = first.GetProperty("scope").GetProperty(key).GetString();
                if (string.IsNullOrEmpty(value) || value != last.GetProperty("scope").GetProperty(key).GetString()) throw new InvalidOperationException("Terminal scope mismatch.");
            }
            return (Hash(path), first.GetProperty("scope").GetProperty("task").GetString()!);
        }
        finally { foreach (var row in rows) row.Dispose(); }
    }
    static string Fingerprint(string workspace, string assembly)
    {
        using var hash = IncrementalHash.CreateHash(HashAlgorithmName.SHA256);
        var paths = new List<string>();
        void Visit(string directory)
        {
            foreach (var file in Directory.EnumerateFiles(directory)) paths.Add(Physical(file, false));
            foreach (var child in Directory.EnumerateDirectories(directory))
                if (!new[] { ".git", "bin", "obj", "node_modules", "artifacts" }.Contains(Path.GetFileName(child))) Visit(Physical(child, true));
        }
        Visit(workspace);
        // Bind model and dependency/resolution metadata separately from source.
        paths.AddRange(Directory.EnumerateFiles(Path.GetDirectoryName(assembly)!).Where(file => file.EndsWith(".dll", StringComparison.OrdinalIgnoreCase) || file.EndsWith(".json", StringComparison.OrdinalIgnoreCase)).Select(file => Physical(file, false)));
        foreach (var file in paths.Distinct().Order(StringComparer.Ordinal))
            hash.AppendData(System.Text.Encoding.UTF8.GetBytes(Path.GetRelativePath(workspace, file).Replace('\\', '/') + "\0" + Hash(file) + "\n"));
        return Convert.ToHexString(hash.GetHashAndReset()).ToLowerInvariant();
    }
    static void Set(object instance, string name, object value)
    {
        var property = instance.GetType().GetProperty(name);
        if (property?.CanWrite == true) property.SetValue(instance, value);
    }
    sealed record Check(string Id, bool Passed);
    sealed class ModelContext(string assembly) : AssemblyLoadContext
    {
        readonly AssemblyDependencyResolver resolver = new(assembly);
        protected override Assembly? Load(AssemblyName name)
        {
            // Share framework/DataAnnotations identity with Validator.
            if (name.Name?.StartsWith("System.", StringComparison.Ordinal) == true) return null;
            var path = resolver.ResolveAssemblyToPath(name);
            return path is null ? null : LoadFromAssemblyPath(path);
        }
    }
}
