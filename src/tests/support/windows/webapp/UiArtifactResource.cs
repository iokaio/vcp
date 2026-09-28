// SPDX-License-Identifier: Apache-2.0
// Disabled in ordinary WEB builds. The trusted builder may replace this staged
// file with one exact, hash-bound, <=64 KiB HTML resource before compilation.
namespace Vcp.Qualification.Webapp {
    public static class UiArtifactResource {
        public static readonly bool Enabled=false;
        public static readonly string CaseId="";
        public static readonly string ArtifactSha256="";
        public static readonly FrozenWebResource Resource=new FrozenWebResource("ui-artifact","/ui-artifact/index.html",200,"text/html; charset=utf-8","");
    }
}
