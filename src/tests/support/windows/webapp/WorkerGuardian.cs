// SPDX-License-Identifier: Apache-2.0
// Controller-owned worker launcher. The worker is atomically placed in an
// unnamed kill-on-close job; it may create the separately owned nested browser
// job after launch.
using System;
using System.Collections.Generic;
using System.ComponentModel;
using System.IO;
using System.Linq;
using System.Runtime.InteropServices;
using System.Text;
using Microsoft.Win32.SafeHandles;

namespace Vcp.Cs3Draft {
public static partial class NativeProbe {
    public sealed class WorkerGuardian : IDisposable {
        readonly object gate = new object();
        IntPtr process;
        IntPtr job;
        bool disposed;
        public StreamWriter StandardInput { get; private set; }
        public StreamReader StandardOutput { get; private set; }
        public StreamReader StandardError { get; private set; }

        internal WorkerGuardian(IntPtr process, IntPtr job, FileStream input, FileStream output, FileStream error) {
            this.process = process;
            this.job = job;
            StandardInput = new StreamWriter(input, Utf8, 4096, false);
            StandardOutput = new StreamReader(output, Utf8, false, 4096, false);
            StandardError = new StreamReader(error, Utf8, false, 4096, false);
        }

        void Open() { if (disposed || process == IntPtr.Zero) throw new ObjectDisposedException(nameof(WorkerGuardian)); }
        public IntPtr Handle { get { lock (gate) { Open(); return process; } } }
        public bool HasExited {
            get {
                lock (gate) {
                    Open();
                    uint result = WaitForSingleObject(process, 0);
                    if (result == 0) return true;
                    if (result == 258) return false;
                    throw new Win32Exception(Marshal.GetLastWin32Error());
                }
            }
        }
        public bool WaitForExit(int milliseconds) {
            if (milliseconds < 0) throw new ArgumentOutOfRangeException(nameof(milliseconds));
            lock (gate) {
                Open();
                uint result = WaitForSingleObject(process, checked((uint)milliseconds));
                if (result == 0) return true;
                if (result == 258) return false;
                throw new Win32Exception(Marshal.GetLastWin32Error());
            }
        }
        public void Kill() {
            lock (gate) {
                Open();
                uint result = WaitForSingleObject(process, 0);
                if (result == 0) return;
                if (result != 258) throw new Win32Exception(Marshal.GetLastWin32Error());
                Check(TerminateJobObject(job, 1));
            }
        }
        public int ExitCode {
            get {
                lock (gate) {
                    Open();
                    uint result = WaitForSingleObject(process, 0);
                    if (result == 258) throw new InvalidOperationException("Worker is still active");
                    if (result != 0) throw new Win32Exception(Marshal.GetLastWin32Error());
                    uint code;
                    Check(GetExitCodeProcess(process, out code));
                    return unchecked((int)code);
                }
            }
        }
        public void Dispose() {
            lock (gate) {
                if (disposed) return;
                disposed = true;
                Exception failure = null;
                Action<Action> cleanup = action => { try { action(); } catch (Exception error) { if (failure == null) failure = error; } };
                // Terminate and drain the guardian before stream disposal. A
                // StreamWriter flush must never delay the owner-loss boundary.
                if (job != IntPtr.Zero) {
                    cleanup(() => Check(TerminateJobObject(job, 1)));
                    cleanup(() => WaitGuardianEmpty(job, 10000));
                    IntPtr ownedJob = job; job = IntPtr.Zero;
                    cleanup(() => Check(CloseHandle(ownedJob)));
                }
                cleanup(() => { if (StandardInput != null) StandardInput.Dispose(); });
                cleanup(() => { if (StandardOutput != null) StandardOutput.Dispose(); });
                cleanup(() => { if (StandardError != null) StandardError.Dispose(); });
                if (process != IntPtr.Zero) {
                    IntPtr ownedProcess = process; process = IntPtr.Zero;
                    cleanup(() => Check(CloseHandle(ownedProcess)));
                }
                if (failure != null) throw failure;
            }
        }
    }

    static string ExactWorkerPath(string value, string leaf) {
        if (String.IsNullOrEmpty(value) || value.IndexOf('\0') >= 0 || !Path.IsPathFullyQualified(value)) throw new ArgumentException("Absolute worker path required");
        string full = Path.GetFullPath(value);
        if (!String.Equals(Path.GetFileName(full), leaf, StringComparison.OrdinalIgnoreCase) || !File.Exists(full)) throw new IOException("Exact worker file unavailable");
        for (string path = full; path != null; path = Path.GetDirectoryName(path)) {
            if ((File.GetAttributes(path) & FileAttributes.ReparsePoint) != 0) throw new IOException("Worker path reparse ancestor rejected");
        }
        return full;
    }

    static SortedDictionary<string,string> WorkerEnvironmentEntries(string[] names, string[] values) {
        if (names == null || values == null || names.Length != values.Length || names.Length > 16) throw new ArgumentException("Bounded exact worker environment required");
        var entries = new SortedDictionary<string,string>(StringComparer.OrdinalIgnoreCase);
        for (int i = 0; i < names.Length; i++) {
            string name = names[i], value = values[i];
            if (String.IsNullOrEmpty(name) || name.IndexOfAny(new[]{'=', '\0'}) >= 0 || value == null || value.IndexOf('\0') >= 0 || entries.ContainsKey(name)) throw new ArgumentException("Invalid worker environment");
            entries.Add(name, value);
        }
        return entries;
    }
    static IntPtr WorkerEnvironment(string[] names, string[] values) {
        var entries = WorkerEnvironmentEntries(names, values);
        string block = String.Join("\0", entries.Select(item => item.Key + "=" + item.Value)) + "\0\0";
        return Marshal.StringToHGlobalUni(block);
    }

    public static WorkerGuardian StartWorker(string executableValue, string scriptValue, string receiptValue, string[] environmentNames, string[] environmentValues) {
        string executable = ExactWorkerPath(executableValue, "pwsh.exe");
        string script = ExactWorkerPath(scriptValue, "Invoke-NativeProbe.ps1");
        string receipt = ExactWorkerPath(receiptValue, "native-receipt.json");
        ValidateWorkerRelationship(script, receipt);
        string root = Path.GetDirectoryName(Path.GetDirectoryName(receipt));
        string[] arguments = { executable, "-NoProfile", "-NonInteractive", "-File", script, "-Mode", "worker", "-Execute", "-Receipt", receipt };
        return LaunchGuardian(executable, arguments, root, environmentNames, environmentValues);
    }

    static void ValidateWorkerRelationship(string script, string receipt) {
        string run = Path.GetDirectoryName(receipt);
        string root = Path.GetDirectoryName(run);
        if (!String.Equals(Path.GetFileName(script), "Invoke-NativeProbe.ps1", StringComparison.OrdinalIgnoreCase) || !String.Equals(Path.GetFileName(receipt), "native-receipt.json", StringComparison.OrdinalIgnoreCase) || !System.Text.RegularExpressions.Regex.IsMatch(Path.GetFileName(run) ?? "", "\\Arun-[a-f0-9]{32}\\z") || !String.Equals(root, Path.GetDirectoryName(script), StringComparison.OrdinalIgnoreCase)) throw new IOException("Worker receipt is outside exact script run directory");
    }

    // Fixed synthetic smoke only: no file, browser, profile, registry or ACL input.
    public static WorkerGuardian StartWorkerGuardianSmoke(string executableValue, string[] environmentNames, string[] environmentValues) {
        string executable = ExactWorkerPath(executableValue, "pwsh.exe");
        const string command = "[Console]::Out.WriteLine($PID); [Console]::Out.Flush(); if (-not [Threading.ManualResetEvent]::new($false).WaitOne(30000)) { throw 'Fixed guardian waiter deadline' }";
        string[] arguments = { executable, "-NoProfile", "-NonInteractive", "-Command", command };
        return LaunchGuardian(executable, arguments, Path.GetDirectoryName(executable), environmentNames, environmentValues);
    }

    static WorkerGuardian LaunchGuardian(string executable, string[] arguments, string workingDirectory, string[] environmentNames, string[] environmentValues) {
        if (arguments == null || arguments.Length == 0 || arguments.Length > 16 || arguments[0] != executable || arguments.Any(value => value == null || value.IndexOf('\0') >= 0)) throw new ArgumentException("Exact bounded worker argv required");
        if (String.IsNullOrEmpty(workingDirectory) || !Path.IsPathFullyQualified(workingDirectory)) throw new ArgumentException("Absolute worker directory required");

        FileStream input = null, output = null, error = null;
        IntPtr childIn = IntPtr.Zero, childOut = IntPtr.Zero, childErr = IntPtr.Zero;
        IntPtr job = IntPtr.Zero, limits = IntPtr.Zero, list = IntPtr.Zero, handles = IntPtr.Zero, jobList = IntPtr.Zero, environment = IntPtr.Zero;
        bool initialized = false;
        var created = new ProcessInfo();
        try {
            input = Pipe(true, out childIn);
            output = Pipe(false, out childOut);
            error = Pipe(false, out childErr);
            job = CreateJobObject(IntPtr.Zero, null); Check(job != IntPtr.Zero);
            // The nested browser job permits at most 32 processes; include its
            // worker parent in this outer controller-owned guardian ceiling.
            limits = Structure(new ExtendedLimits { Basic = new BasicLimits { Flags = 0x2008, ActiveProcesses = 33 } });
            Check(SetInformationJobObject(job, 9, limits, (uint)Marshal.SizeOf<ExtendedLimits>()));

            IntPtr size = IntPtr.Zero;
            InitializeProcThreadAttributeList(IntPtr.Zero, 2, 0, ref size);
            if (size == IntPtr.Zero) throw new Win32Exception(Marshal.GetLastWin32Error());
            list = Marshal.AllocHGlobal(size);
            Check(InitializeProcThreadAttributeList(list, 2, 0, ref size)); initialized = true;
            handles = Marshal.AllocHGlobal(3 * IntPtr.Size);
            Marshal.Copy(new[]{childIn, childOut, childErr}, 0, handles, 3);
            Check(UpdateProcThreadAttribute(list, 0, new IntPtr(0x20002), handles, new IntPtr(3 * IntPtr.Size), IntPtr.Zero, IntPtr.Zero));
            jobList = Marshal.AllocHGlobal(IntPtr.Size); Marshal.WriteIntPtr(jobList, job);
            Check(UpdateProcThreadAttribute(list, 0, new IntPtr(0x2000d), jobList, new IntPtr(IntPtr.Size), IntPtr.Zero, IntPtr.Zero));
            environment = WorkerEnvironment(environmentNames, environmentValues);

            var startup = new StartupEx { Startup = new Startup { Size = Marshal.SizeOf<StartupEx>(), Flags = 0x100, Input = childIn, Output = childOut, Error = childErr }, Attributes = list };
            const uint flags = 0x08080404u; // NO_WINDOW | UNICODE_ENV | EXTENDED_STARTUPINFO | SUSPENDED.
            Check(CreateProcess(executable, new StringBuilder(String.Join(" ", arguments.Select(Quote))), IntPtr.Zero, IntPtr.Zero, true, flags, environment, workingDirectory, ref startup, out created));
            bool member; Check(IsProcessInJob(created.Process, job, out member));
            if (!member) throw new IOException("Worker was not atomically assigned to guardian job");
            Check(CloseHandle(childIn)); childIn = IntPtr.Zero;
            Check(CloseHandle(childOut)); childOut = IntPtr.Zero;
            Check(CloseHandle(childErr)); childErr = IntPtr.Zero;
            if (ResumeThread(created.Thread) == UInt32.MaxValue) throw new Win32Exception(Marshal.GetLastWin32Error());
            Check(CloseHandle(created.Thread)); created.Thread = IntPtr.Zero;

            var guardian = new WorkerGuardian(created.Process, job, input, output, error);
            created.Process = IntPtr.Zero; job = IntPtr.Zero; input = output = error = null;
            return guardian;
        } catch (Exception launchFailure) {
            Exception cleanupFailure = null;
            try {
                if (job != IntPtr.Zero) {
                    Check(TerminateJobObject(job, 1));
                    WaitGuardianEmpty(job, 10000);
                } else if (created.Process != IntPtr.Zero) {
                    Check(TerminateProcess(created.Process, 1));
                    uint result = WaitForSingleObject(created.Process, 10000);
                    if (result == 258) throw new TimeoutException("Unassigned worker termination deadline");
                    if (result != 0) throw new Win32Exception(Marshal.GetLastWin32Error());
                }
            } catch (Exception cleanupError) { cleanupFailure = cleanupError; }
            if (cleanupFailure != null) throw new AggregateException("Guardian launch failed and descendant drainage was not established", launchFailure, cleanupFailure);
            throw;
        } finally {
            foreach (IntPtr handle in new[]{childIn, childOut, childErr, created.Process, created.Thread}) if (handle != IntPtr.Zero) CloseHandle(handle);
            if (initialized) DeleteProcThreadAttributeList(list);
            foreach (IntPtr pointer in new[]{list, handles, jobList, limits, environment}) if (pointer != IntPtr.Zero) Marshal.FreeHGlobal(pointer);
            if (input != null) input.Dispose();
            if (output != null) output.Dispose();
            if (error != null) error.Dispose();
            if (job != IntPtr.Zero) CloseHandle(job);
        }
    }

    static void WaitGuardianEmpty(IntPtr job, int milliseconds) {
        var clock = System.Diagnostics.Stopwatch.StartNew();
        while (Accounts(job).Active != 0) {
            if (clock.ElapsedMilliseconds >= milliseconds) throw new TimeoutException("Guardian job drainage deadline");
            System.Threading.Thread.Sleep(10);
        }
    }

    // Pure validation only; called by compile-only mode.
    public static int TestWorkerGuardianContract() {
        int checks = 0;
        Action<bool> check = value => { if (!value) throw new Exception("Worker guardian contract assertion failed"); checks++; };
        Action<Action> reject = action => { bool failed = false; try { action(); } catch { failed = true; } check(failed); };
        var env = WorkerEnvironmentEntries(new[]{"WINDIR","TEMP"}, new[]{"windows","temp"});
        check(env.Count == 2 && env.Keys.First() == "TEMP" && env.Keys.Last() == "WINDIR");
        reject(() => WorkerEnvironmentEntries(new[]{"Path","PATH"}, new[]{"a","b"}));
        reject(() => WorkerEnvironmentEntries(new[]{"BAD=NAME"}, new[]{"value"}));
        reject(() => WorkerEnvironmentEntries(new[]{"TEMP"}, new[]{"bad\0value"}));
        reject(() => WorkerEnvironmentEntries(new string[17], new string[17]));
        string root = Path.Combine("C:\\", "fixture");
        string script = Path.Combine(root, "Invoke-NativeProbe.ps1");
        string receipt = Path.Combine(root, "run-" + new string('a',32), "native-receipt.json");
        ValidateWorkerRelationship(script, receipt); checks++;
        reject(() => ValidateWorkerRelationship(script, Path.Combine(root, "run-short", "native-receipt.json")));
        reject(() => ValidateWorkerRelationship(Path.Combine(root,"Other.ps1"), receipt));
        reject(() => ValidateWorkerRelationship(script, Path.Combine("C:\\", "other", "run-"+new string('a',32), "native-receipt.json")));
        return checks;
    }
}
}
