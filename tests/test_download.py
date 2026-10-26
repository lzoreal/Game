import os
import sys
import unittest
from unittest import mock

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

import catalog as core


class FakeResponse:
    def __init__(self, status_code=200, reason="OK", headers=None, content=b"{}"):
        self.status_code = status_code
        self.reason = reason
        self.headers = headers or {}
        self.content = content


class FakeCurlModule:
    def __init__(self, responses):
        self.responses = list(responses)
        self.calls = []

    def get(self, url, **kwargs):
        self.calls.append(kwargs)
        if not self.responses:
            raise AssertionError("unexpected extra request")
        return self.responses.pop(0)


CHALLENGE = FakeResponse(
    status_code=403,
    reason="Forbidden",
    headers={"Server": "cloudflare", "cf-mitigated": "challenge"},
    content=b"<html><title>Just a moment...</title></html>",
)


class ChallengeDetectionTests(unittest.TestCase):
    def test_detects_cloudflare_status_and_body(self):
        self.assertTrue(
            core.is_cloudflare_challenge(
                403, {"Server": "cloudflare"}, b"<title>Just a moment...</title>"
            )
        )
        self.assertTrue(
            core.is_cloudflare_challenge(
                503, {}, b"<div id='cf-chl-widget'>checking your browser</div>"
            )
        )
        self.assertTrue(
            core.is_cloudflare_challenge(429, {"cf-mitigated": "challenge"}, b"")
        )

    def test_ignores_plain_and_successful_responses(self):
        self.assertFalse(core.is_cloudflare_challenge(403, {}, b'{"error":"nope"}'))
        self.assertFalse(
            core.is_cloudflare_challenge(200, {}, b"Just a moment...")
        )
        self.assertFalse(core.is_cloudflare_challenge(404, {}, b""))


class ImpersonatingDownloadTests(unittest.TestCase):
    def download(self, module, url="https://mirror.example/list.json"):
        with mock.patch.object(core, "curl_requests", return_value=module):
            return core.Downloader().download_json(url)

    def test_retries_with_another_browser_fingerprint(self):
        module = FakeCurlModule(
            [CHALLENGE, FakeResponse(content='{"downloads": []}'.encode("utf-8"))]
        )
        content = self.download(module)
        self.assertEqual(content, '{"downloads": []}')
        self.assertEqual(
            [call["impersonate"] for call in module.calls], ["chrome", "safari"]
        )
        self.assertTrue(all(call["verify"] is False for call in module.calls))

    def test_raises_friendly_error_when_every_profile_is_blocked(self):
        module = FakeCurlModule([CHALLENGE, CHALLENGE, CHALLENGE])
        with self.assertRaises(core.CloudflareBlocked) as ctx:
            self.download(module)
        message = str(ctx.exception)
        self.assertIn("Cloudflare 人机验证", message)
        self.assertIn("chrome", message)
        self.assertEqual(len(module.calls), len(core.IMPERSONATE_PROFILES))

    def test_reports_http_errors_without_retrying(self):
        module = FakeCurlModule([FakeResponse(status_code=404, reason="Not Found")])
        with self.assertRaises(RuntimeError) as ctx:
            self.download(module)
        self.assertIn("HTTP 404", str(ctx.exception))
        self.assertEqual(len(module.calls), 1)

    def test_reports_transport_errors(self):
        class Broken:
            def get(self, url, **kwargs):
                raise RuntimeError("connection reset")

        with self.assertRaises(RuntimeError) as ctx:
            self.download(Broken())
        self.assertIn("下载失败", str(ctx.exception))
        self.assertIn("connection reset", str(ctx.exception))


class UrllibFallbackTests(unittest.TestCase):
    def test_uses_urllib_when_curl_cffi_is_missing(self):
        response = mock.MagicMock()
        response.read.return_value = b'{"downloads": []}'
        response.info.return_value.get.return_value = None
        response.__enter__.return_value = response
        response.__exit__.return_value = False

        with mock.patch.object(core, "curl_requests", return_value=None):
            with mock.patch.object(
                core.urllib.request, "urlopen", return_value=response
            ) as urlopen:
                content = core.Downloader().download_json("https://example.test/a.json")

        self.assertEqual(content, '{"downloads": []}')
        request = urlopen.call_args.args[0]
        self.assertEqual(request.full_url, "https://example.test/a.json")
        self.assertEqual(request.get_header("Referer"), "https://example.test/")

    def test_urllib_challenge_reports_cloudflare_hint(self):
        error = core.urllib.error.HTTPError(
            "https://example.test/a.json",
            403,
            "Forbidden",
            {"Server": "cloudflare"},
            None,
        )
        self.addCleanup(error.close)
        error.read = lambda size=8192: b"<title>Just a moment...</title>"

        with mock.patch.object(core, "curl_requests", return_value=None):
            with mock.patch.object(
                core.urllib.request, "urlopen", side_effect=error
            ):
                with self.assertRaises(core.CloudflareBlocked) as ctx:
                    core.Downloader().download_json("https://example.test/a.json")

        self.assertIn("Cloudflare", str(ctx.exception))


if __name__ == "__main__":
    unittest.main()
