# Explicit native encrypted publisher

The `backup/publisher/1` API publishes a whole-workspace encrypted backup to an
already configured **local vault**. It does not prove cloud transfer or restore.
Existing native `backup keys` and `backup configure` commands establish independent
recovery enrollment, staging and vault configuration. Public API calls cannot
enroll keys, select filesystem destinations or change automatic scheduling.

Select a host-owned profile outside repositories, canonical data, staging, vaults
and synchronization roots. The strict JSON file is at most 16 KiB:

```json
{
  "version": 1,
  "key": "C:\\private\\recovery\\KEY_REFERENCE.recovery",
  "git": "C:\\Program Files\\Git\\bin\\git.exe"
}
```

Both references must be absolute local paths. The key must be the independently
verified recovery copy matching the current enrollment. The executable loader
rejects redirected paths and hard-linked authority files, bounds the profile
read, and retains a native Git file pin for the loaded capability and any
background capture using it. The key file remains pinned through import and
verification. Key bytes remain in native verified handles. The profile contains references, not private-key
material; do not create it through workspace-controlled configuration.

The SDK selects this profile only during an explicit controller launch:

```ts
const client = await launchLocal({
  executable, workspace, data, role: 'controller', transport: 'windows_pipe',
  publisher: { profile: 'C:\\private\\publisher.json' },
});
```

The Git executable must have a single filesystem link. Some Git for Windows
installations hard-link `cmd/git.exe` to another executable; that path is rejected.
Select an independently verified single-link executable explicitly, such as
`bin/git.exe` when that installation provides it. The loader never substitutes
a different executable.

Selection requires negotiation of `backup/publisher/1` and `workspace/binding/1`
for exact root guards. Omitting it preserves the
ordinary bootstrap and older-engine compatibility. An explicitly selected profile
against an older engine fails closed; the SDK does not silently retry without it.
Loading a missing or invalid profile leaves ordinary inspection usable with the
publisher unavailable. No native error text, key path or vault path is exposed in
public status. Explicit loading disables automatic scheduling for this server
without rewriting the independent configuration.

Acquire control explicitly before `backup/create`, `backup/retry` or
`backup/cancel`. Create uses the caller's UUID command identity as its operation
identity. Acceptance means the intent is durable; use `command/read` to reconcile
that acceptance and `backup/read` to observe publication, checkpoint and cleanup.
A timeout does not prove cancellation. Retry is a new deliberate command against
the original operation and its current revisions.

Observer reconnection carries no profile selection and reloads no keys. An
observer can inspect authorized existing operation metadata without taking another
client's controller ownership. A new server has no loaded capability until an
explicit native launch selects one. Status polling never retries, resumes or
loads keys. Local publication still reports cloud transfer `unknown` and restore
verification `not_observed`.
