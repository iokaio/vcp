// SPDX-License-Identifier: Apache-2.0
using System;
using System.Text;

namespace Vcp.Qualification.Webapp
{
    public enum WebDomPhase
    {
        Normal,
        CrossOriginNegative,
        Complete,
        Failed
    }

    public sealed class WebDomResource
    {
        internal WebDomResource(string url, string contentType, string body)
        {
            Url = url;
            ContentType = contentType;
            Body = body;
            Utf8Bytes = Encoding.UTF8.GetByteCount(body);
        }

        public string Url { get; private set; }
        public string ContentType { get; private set; }
        public string Body { get; private set; }
        public int Utf8Bytes { get; private set; }
    }

    public static class WebDomContract
    {
        public const string FormUrl = "https://cs3-fixture.invalid/form.html";
        public const string ScriptUrl = "https://cs3-fixture.invalid/form.js";
        public const string BlockedUrl = "https://blocked.invalid/";
        public const int MaxResourceCount = 2;
        public const int MaxResourceBytes = 64 * 1024;
        public const int MaxCumulativeBytes = MaxResourceCount * MaxResourceBytes;

        public static readonly string FormHtml =
            "<!doctype html>\n" +
            "<html lang=\"en\">\n" +
            "<head>\n" +
            "<meta charset=\"utf-8\">\n" +
            "<meta http-equiv=\"Content-Security-Policy\" content=\"default-src 'none'; script-src 'self'; connect-src 'none'; img-src 'none'; style-src 'none'; form-action 'none'\">\n" +
            "<title>CS-3 form fixture</title>\n" +
            "</head>\n" +
            "<body>\n" +
            "<main>\n" +
            "<h1>Profile</h1>\n" +
            "<form id=\"profile\" novalidate>\n" +
            "<label for=\"name\">Name</label>\n" +
            "<input id=\"name\" name=\"name\" required aria-describedby=\"name-error\">\n" +
            "<p id=\"name-error\" role=\"alert\"></p>\n" +
            "<button type=\"submit\">Save</button>\n" +
            "</form>\n" +
            "<p id=\"status\" role=\"status\"></p>\n" +
            "</main>\n" +
            "<script src=\"form.js\"></script>\n" +
            "</body>\n" +
            "</html>\n";

        public static readonly string FormJavaScript =
            "'use strict';\n" +
            "(() => {\n" +
            "  const form = document.getElementById('profile');\n" +
            "  const name = document.getElementById('name');\n" +
            "  const error = document.getElementById('name-error');\n" +
            "  const status = document.getElementById('status');\n" +
            "  const evidence = window.__cs3InputEvidence = {scriptReady:false,keyDowns:0,keyPresses:0,keyUps:0,submits:0,lastKey:'',trustedKeys:true,readinessDowns:0,readinessKeyPresses:0,readinessUps:0,readinessRepeats:0,readinessSequence:'',readinessTrusted:true};\n" +
            "  const readiness = (field, event) => {\n" +
            "    if (event.key !== 'Escape' || event.code !== 'Escape') return false;\n" +
            "    if (field === 'keyDowns') evidence.readinessDowns = Math.min(16, evidence.readinessDowns + 1);\n" +
            "    else if (field === 'keyPresses') evidence.readinessKeyPresses = Math.min(16, evidence.readinessKeyPresses + 1);\n" +
            "    else if (field === 'keyUps') evidence.readinessUps = Math.min(16, evidence.readinessUps + 1);\n" +
            "    evidence.readinessRepeats = Math.min(16, evidence.readinessRepeats + (event.repeat === true ? 1 : 0));\n" +
            "    evidence.readinessSequence = (evidence.readinessSequence + (field === 'keyDowns' ? 'D' : field === 'keyPresses' ? 'P' : 'U')).slice(0, 16);\n" +
            "    evidence.readinessTrusted = evidence.readinessTrusted && event.isTrusted === true;\n" +
            "    event.preventDefault(); event.stopImmediatePropagation(); return true;\n" +
            "  };\n" +
            "  const key = (field, event) => { if (readiness(field, event)) return; evidence[field] = Math.min(16, evidence[field] + 1); evidence.lastKey = String(event.key || '').slice(0, 16); evidence.trustedKeys = evidence.trustedKeys && event.isTrusted === true; };\n" +
            "  document.addEventListener('keydown', event => key('keyDowns', event), true);\n" +
            "  document.addEventListener('keypress', event => key('keyPresses', event), true);\n" +
            "  document.addEventListener('keyup', event => key('keyUps', event), true);\n" +
            "  name.addEventListener('input', () => { error.textContent = ''; status.textContent = ''; });\n" +
            "  form.addEventListener('submit', event => {\n" +
            "    evidence.submits = Math.min(16, evidence.submits + 1);\n" +
            "    event.preventDefault();\n" +
            "    const value = name.value.trim();\n" +
            "    if (!value) {\n" +
            "      status.textContent = '';\n" +
            "      error.textContent = 'Name is required.';\n" +
            "      name.focus();\n" +
            "      return;\n" +
            "    }\n" +
            "    error.textContent = '';\n" +
            "    status.textContent = 'Saved ' + value + '.';\n" +
            "  });\n" +
            "  evidence.scriptReady = true;\n" +
            "})();\n";

        private static readonly WebDomResource FormResource =
            new WebDomResource(FormUrl, "text/html; charset=utf-8", FormHtml);
        private static readonly WebDomResource ScriptResource =
            new WebDomResource(ScriptUrl, "text/javascript; charset=utf-8", FormJavaScript);

        static WebDomContract()
        {
            if (FormResource.Utf8Bytes <= 0 || FormResource.Utf8Bytes > MaxResourceBytes ||
                ScriptResource.Utf8Bytes <= 0 || ScriptResource.Utf8Bytes > MaxResourceBytes ||
                FormResource.Utf8Bytes + ScriptResource.Utf8Bytes > MaxCumulativeBytes)
            {
                throw new InvalidOperationException("Embedded web fixture exceeds its byte contract");
            }
        }

        public static WebDomSession CreateSession()
        {
            return new WebDomSession(FormResource, ScriptResource);
        }
    }

    public sealed class WebDomSession
    {
        private readonly WebDomResource form;
        private readonly WebDomResource script;
        private int nextResource;
        private bool deniedNavigationObserved;
        private string failure;

        internal WebDomSession(WebDomResource form, WebDomResource script)
        {
            this.form = form;
            this.script = script;
            Phase = WebDomPhase.Normal;
        }

        public WebDomPhase Phase { get; private set; }
        public bool Failed { get { return Phase == WebDomPhase.Failed; } }
        public string Failure { get { return failure; } }
        public int ResourceCount { get; private set; }
        public int CumulativeBytes { get; private set; }

        public bool TryServe(string method, string rawUrl, out WebDomResource resource)
        {
            resource = null;
            if (Failed) return false;
            if (Phase != WebDomPhase.Normal) return LatchFailure("Resource requested outside the normal phase");
            if (!String.Equals(method, "GET", StringComparison.Ordinal)) return LatchFailure("Only exact GET resource requests are permitted");

            WebDomResource expected = nextResource == 0 ? form : nextResource == 1 ? script : null;
            if (expected == null) return LatchFailure("Resource count exceeded");
            if (!String.Equals(rawUrl, expected.Url, StringComparison.Ordinal)) return LatchFailure("Unknown or non-exact resource URL");
            if (expected.Utf8Bytes <= 0 || expected.Utf8Bytes > WebDomContract.MaxResourceBytes ||
                ResourceCount + 1 > WebDomContract.MaxResourceCount ||
                CumulativeBytes > WebDomContract.MaxCumulativeBytes - expected.Utf8Bytes)
            {
                return LatchFailure("Resource bounds exceeded");
            }

            resource = expected;
            nextResource++;
            ResourceCount++;
            CumulativeBytes += expected.Utf8Bytes;
            return true;
        }

        public bool BeginNegativePhase()
        {
            if (Failed) return false;
            if (Phase != WebDomPhase.Normal || nextResource != WebDomContract.MaxResourceCount)
                return LatchFailure("Negative phase began before the exact normal resource sequence completed");
            Phase = WebDomPhase.CrossOriginNegative;
            return true;
        }

        public bool ObserveDeniedNavigation(string rawUrl, bool denied)
        {
            if (Failed) return false;
            if (Phase != WebDomPhase.CrossOriginNegative) return LatchFailure("Denied navigation observed outside the negative phase");
            if (deniedNavigationObserved) return LatchFailure("More than one denied navigation was observed");
            deniedNavigationObserved = true;
            if (!String.Equals(rawUrl, WebDomContract.BlockedUrl, StringComparison.Ordinal))
                return LatchFailure("Unknown or non-exact negative navigation URL");
            if (!denied) return LatchFailure("Expected cross-origin navigation was not denied");
            return true;
        }

        public bool Complete()
        {
            if (Failed) return false;
            if (Phase != WebDomPhase.CrossOriginNegative || !deniedNavigationObserved)
                return LatchFailure("Completion occurred before the exact negative observation");
            Phase = WebDomPhase.Complete;
            return true;
        }

        public bool LatchFailure(string reason)
        {
            if (!Failed)
            {
                failure = String.IsNullOrWhiteSpace(reason) ? "Caller reported an unspecified contract failure" : reason;
                Phase = WebDomPhase.Failed;
            }
            return false;
        }
    }
}
