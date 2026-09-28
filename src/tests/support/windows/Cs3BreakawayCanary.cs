// SPDX-License-Identifier: Apache-2.0
// Bounded no-breakaway negative control; never accepts an arbitrary executable.
using System;
using System.Diagnostics;
using System.Runtime.InteropServices;
using System.Text;
internal static class Cs3BreakawayCanary {
    [StructLayout(LayoutKind.Sequential, CharSet=CharSet.Unicode)]
    struct Startup { public uint Size; public string Reserved, Desktop, Title; public uint X,Y,Width,Height,CharsX,CharsY,Fill,Flags; public ushort Show, ReservedSize; public IntPtr ReservedBytes, Input,Output,Error; }
    [StructLayout(LayoutKind.Sequential)]
    struct ProcessInfo { public IntPtr Process,Thread; public uint Pid,Tid; }
    [DllImport("kernel32.dll", CharSet=CharSet.Unicode, SetLastError=true)]
    static extern bool CreateProcess(string app,StringBuilder command,IntPtr pa,IntPtr ta,bool inherit,uint flags,IntPtr environment,string cwd,ref Startup startup,out ProcessInfo process);
    [DllImport("kernel32.dll",SetLastError=true)] static extern bool TerminateProcess(IntPtr process,uint code);
    [DllImport("kernel32.dll")] static extern uint WaitForSingleObject(IntPtr process,uint milliseconds);
    [DllImport("kernel32.dll")] static extern bool CloseHandle(IntPtr handle);
    [DllImport("kernel32.dll",SetLastError=true)] static extern bool QueryInformationJobObject(IntPtr job,int kind,IntPtr info,uint bytes,IntPtr returned);
    static int Main(string[] args) {
        if(args.Length==1 && args[0]=="child") return 91;
        if(args.Length!=0 || IntPtr.Size!=8) return 2;
        IntPtr limits=Marshal.AllocHGlobal(144);
        try {
            if(!QueryInformationJobObject(IntPtr.Zero,9,limits,144,IntPtr.Zero)) return 3;
            uint flags=unchecked((uint)Marshal.ReadInt32(limits,16));
            if((flags & 0x1800)!=0 || (flags & 0x2000)==0) return 4;
            string executable=Process.GetCurrentProcess().MainModule.FileName;
            var startup=new Startup {Size=(uint)Marshal.SizeOf(typeof(Startup))};
            ProcessInfo child;
            // Suspended: even an unexpected grant cannot execute a child action.
            bool created=CreateProcess(executable,new StringBuilder("\""+executable+"\" child"),IntPtr.Zero,IntPtr.Zero,false,0x01000004,IntPtr.Zero,null,ref startup,out child);
            int error=Marshal.GetLastWin32Error();
            if(created) {
                try { if(!TerminateProcess(child.Process,92) || WaitForSingleObject(child.Process,5000)!=0) return 5; }
                finally { CloseHandle(child.Thread);CloseHandle(child.Process); }
                return 6;
            }
            Console.Write("{\"schema\":\"cs3-breakaway-canary/1\",\"job_flags\":"+flags+",\"created\":false,\"win32_error\":"+error+"}");
            return error==5?0:7;
        } finally {Marshal.FreeHGlobal(limits);}
    }
}
