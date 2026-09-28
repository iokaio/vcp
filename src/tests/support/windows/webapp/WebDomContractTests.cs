// SPDX-License-Identifier: Apache-2.0
using System;
using System.Collections.Generic;
using System.Text;
using Vcp.Qualification.Webapp;

internal static class WebDomContractTests
{
    private static int assertions;

    private static void True(bool value, string message)
    {
        assertions++;
        if (!value) throw new Exception(message);
    }

    private static void Equal<T>(T expected, T actual, string message)
    {
        assertions++;
        if (!EqualityComparer<T>.Default.Equals(expected, actual))
            throw new Exception(message + ": expected " + expected + ", actual " + actual);
    }

    private static WebDomSession ServeNormal()
    {
        WebDomResource resource;
        WebDomSession session = WebDomContract.CreateSession();
        True(session.TryServe("GET", WebDomContract.FormUrl, out resource), "exact form route must resolve");
        Equal(WebDomContract.FormUrl, resource.Url, "form URL");
        Equal("text/html; charset=utf-8", resource.ContentType, "form content type");
        Equal(WebDomContract.FormHtml, resource.Body, "form bytes must be the embedded original");
        True(session.TryServe("GET", WebDomContract.ScriptUrl, out resource), "exact script route must resolve");
        Equal(WebDomContract.ScriptUrl, resource.Url, "script URL");
        Equal("text/javascript; charset=utf-8", resource.ContentType, "script content type");
        Equal(WebDomContract.FormJavaScript, resource.Body, "script bytes must be the embedded original");
        return session;
    }

    private static void ExactRoutesRejectAliases()
    {
        string[] variants = new string[] {
            "http://cs3-fixture.invalid/form.html",
            "https://CS3-fixture.invalid/form.html",
            "https://cs3-fixture.invalid:443/form.html",
            "https://user@cs3-fixture.invalid/form.html",
            "https://cs3-fixture.invalid/form.html?x=1",
            "https://cs3-fixture.invalid/form.html#fragment",
            "https://cs3-fixture.invalid/Form.html",
            "https://cs3-fixture.invalid/%66orm.html",
            "https://cs3-fixture.invalid//form.html",
            "https://cs3-fixture.invalid/form.html/",
            "/form.html"
        };
        foreach (string variant in variants)
        {
            WebDomResource ignored;
            WebDomSession session = WebDomContract.CreateSession();
            True(!session.TryServe("GET", variant, out ignored), "route alias must be denied: " + variant);
            True(session.Failed, "route alias must latch failure");
        }

        foreach (string method in new string[] { "get", "POST", "HEAD", "GET ", "" })
        {
            WebDomResource ignored;
            WebDomSession session = WebDomContract.CreateSession();
            True(!session.TryServe(method, WebDomContract.FormUrl, out ignored), "non-exact method must be denied");
            True(session.Failed, "method failure must latch");
        }
    }

    private static void EmbeddedResourcesAreBoundedAndFunctional()
    {
        int htmlBytes = Encoding.UTF8.GetByteCount(WebDomContract.FormHtml);
        int scriptBytes = Encoding.UTF8.GetByteCount(WebDomContract.FormJavaScript);
        True(htmlBytes > 0 && htmlBytes <= WebDomContract.MaxResourceBytes, "HTML byte bound");
        True(scriptBytes > 0 && scriptBytes <= WebDomContract.MaxResourceBytes, "script byte bound");
        True(htmlBytes + scriptBytes <= WebDomContract.MaxCumulativeBytes, "cumulative embedded byte bound");
        True(WebDomContract.FormHtml.Contains("<label for=\"name\">Name</label>"), "programmatic label");
        True(WebDomContract.FormHtml.Contains("name=\"name\" required"), "named required input");
        True(WebDomContract.FormHtml.Contains("role=\"alert\""), "custom error live role");
        True(WebDomContract.FormHtml.Contains("role=\"status\""), "success live role");
        True(WebDomContract.FormJavaScript.Contains("event.preventDefault();"), "submit prevents reload");
        True(WebDomContract.FormJavaScript.Contains("name.focus();"), "invalid submit focuses input");
        True(WebDomContract.FormJavaScript.Contains("Name is required."), "empty-name error");
        True(WebDomContract.FormJavaScript.Contains("'Saved ' + value + '.'"), "named success");
        True(WebDomContract.FormJavaScript.Contains("keyDowns:0,keyPresses:0,keyUps:0,submits:0,lastKey:'',trustedKeys:true"), "bounded ordinary input evidence initialized");
        True(WebDomContract.FormJavaScript.Contains("readinessDowns:0,readinessKeyPresses:0,readinessUps:0,readinessRepeats:0,readinessSequence:'',readinessTrusted:true"), "bounded readiness evidence initialized");
        True(WebDomContract.FormJavaScript.Contains("event.key !== 'Escape' || event.code !== 'Escape'"), "only exact Escape is intercepted");
        True(WebDomContract.FormJavaScript.Contains("evidence.readinessSequence + (field === 'keyDowns' ? 'D' : field === 'keyPresses' ? 'P' : 'U')"), "readiness ordering is retained");
        True(WebDomContract.FormJavaScript.Contains("event.repeat === true ? 1 : 0"), "readiness repeats are retained");
        True(WebDomContract.FormJavaScript.Contains("evidence.readinessTrusted = evidence.readinessTrusted && event.isTrusted === true"), "readiness trust is retained");
        True(WebDomContract.FormJavaScript.Contains("event.preventDefault(); event.stopImmediatePropagation(); return true;"), "readiness key is suppressed before ordinary handling");
        int readinessHandler = WebDomContract.FormJavaScript.IndexOf("const readiness =", StringComparison.Ordinal);
        int ordinaryHandler = WebDomContract.FormJavaScript.IndexOf("const key =", StringComparison.Ordinal);
        True(readinessHandler >= 0 && ordinaryHandler > readinessHandler, "readiness interception precedes ordinary accounting");
        True(WebDomContract.FormJavaScript.Contains("Math.min(16, evidence[field] + 1)"), "key counters capped");
        True(WebDomContract.FormJavaScript.Contains("Math.min(16, evidence.submits + 1)"), "submit counter capped");
        True(WebDomContract.FormJavaScript.Contains("event.isTrusted === true"), "trusted input is observed");
        True(WebDomContract.FormJavaScript.Contains("document.addEventListener('keydown'"), "document keydown observed");
        True(WebDomContract.FormJavaScript.Contains("document.addEventListener('keypress'"), "document keypress observed");
        True(WebDomContract.FormJavaScript.Contains("document.addEventListener('keyup'"), "document keyup observed");
        int submitListener = WebDomContract.FormJavaScript.IndexOf("form.addEventListener('submit'", StringComparison.Ordinal);
        int readyMarker = WebDomContract.FormJavaScript.IndexOf("evidence.scriptReady = true", StringComparison.Ordinal);
        True(submitListener >= 0 && readyMarker > submitListener, "readiness follows submit listener registration");
        True(!WebDomContract.FormJavaScript.Contains(".click()") && !WebDomContract.FormJavaScript.Contains("dispatchEvent(") && !WebDomContract.FormJavaScript.Contains("requestSubmit("), "instrumentation does not synthesize actions");

        WebDomSession session = ServeNormal();
        Equal(WebDomContract.MaxResourceCount, session.ResourceCount, "exact resource count");
        Equal(htmlBytes + scriptBytes, session.CumulativeBytes, "observed cumulative bytes");
        WebDomResource ignored;
        True(!session.TryServe("GET", WebDomContract.ScriptUrl, out ignored), "third resource must fail");
        True(session.Failed, "resource count failure must latch");
    }

    private static void ExactPhaseSequenceAndFailurePersistence()
    {
        WebDomSession early = WebDomContract.CreateSession();
        True(!early.BeginNegativePhase(), "negative phase cannot begin before normal resources");
        Equal(WebDomPhase.Failed, early.Phase, "early transition failure");
        string first = early.Failure;
        True(!early.LatchFailure("replacement"), "failure remains false");
        Equal(first, early.Failure, "first failure reason is retained");
        True(!early.Complete(), "failed state cannot complete");
        Equal(first, early.Failure, "later transition cannot replace failure");

        WebDomSession correct = ServeNormal();
        True(correct.BeginNegativePhase(), "normal to negative transition");
        Equal(WebDomPhase.CrossOriginNegative, correct.Phase, "negative phase");
        True(correct.ObserveDeniedNavigation(WebDomContract.BlockedUrl, true), "one exact denied navigation");
        True(correct.Complete(), "negative to complete transition");
        Equal(WebDomPhase.Complete, correct.Phase, "complete phase");
        True(!correct.Complete(), "duplicate completion is a sequence failure");
        True(correct.Failed, "duplicate completion latches failure");

        WebDomSession duplicate = ServeNormal();
        True(duplicate.BeginNegativePhase(), "duplicate fixture enters negative phase");
        True(duplicate.ObserveDeniedNavigation(WebDomContract.BlockedUrl, true), "first denial accepted");
        True(!duplicate.ObserveDeniedNavigation(WebDomContract.BlockedUrl, true), "second denial rejected");
        True(duplicate.Failed, "second denial latches failure");

        WebDomSession allowed = ServeNormal();
        True(allowed.BeginNegativePhase(), "allowed fixture enters negative phase");
        True(!allowed.ObserveDeniedNavigation(WebDomContract.BlockedUrl, false), "allowed cross-origin navigation fails");
        True(allowed.Failed, "non-denial latches failure");

        foreach (string variant in new string[] {
            "https://blocked.invalid/?x=1",
            "https://blocked.invalid/#fragment",
            "https://BLOCKED.invalid/",
            "https://blocked.invalid:443/",
            "https://user@blocked.invalid/",
            "http://blocked.invalid/",
            "https://other.invalid/"
        })
        {
            WebDomSession unknown = ServeNormal();
            True(unknown.BeginNegativePhase(), "unknown fixture enters negative phase");
            True(!unknown.ObserveDeniedNavigation(variant, true), "non-exact blocked URL rejected: " + variant);
            True(unknown.Failed, "unknown negative attempt latches failure");
        }

        WebDomSession caller = ServeNormal();
        True(caller.BeginNegativePhase(), "caller fixture enters negative phase");
        True(!caller.LatchFailure("Caller observed an unknown request"), "caller can latch an unknown attempt");
        Equal("Caller observed an unknown request", caller.Failure, "caller failure retained");
        True(!caller.ObserveDeniedNavigation(WebDomContract.BlockedUrl, true), "failed caller cannot recover");
        Equal("Caller observed an unknown request", caller.Failure, "failed caller reason remains stable");
    }

    public static int Main()
    {
        try
        {
            ExactRoutesRejectAliases();
            EmbeddedResourcesAreBoundedAndFunctional();
            ExactPhaseSequenceAndFailurePersistence();
            Console.WriteLine("WebDomContractTests passed: " + assertions + " assertions");
            return 0;
        }
        catch (Exception error)
        {
            Console.Error.WriteLine(error.ToString());
            return 1;
        }
    }
}
