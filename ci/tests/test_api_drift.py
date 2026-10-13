"""Tests for ci/api_drift.py and ci/drift_issue.py.

Run with: python3 -m unittest discover -s ci/tests -v

The fixtures under ci/tests/fixtures mirror the real layouts (discord-api-docs
MDX, the crate's REST and decoder sources) with one example of every shape
the extractor must accept or reject, so a regression in the parser shows up
here instead of as a bogus weekly drift issue.
"""

from __future__ import annotations

import contextlib
import io
import json
import shutil
import sys
import tempfile
import unittest
from pathlib import Path

CI_DIR = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(CI_DIR))

import api_drift  # noqa: E402
import drift_issue  # noqa: E402

FIXTURES = Path(__file__).resolve().parent / "fixtures"
DOCS = FIXTURES / "docs"
CRATE = FIXTURES / "crate"
ALLOWLIST = FIXTURES / "allowlist.toml"


def load_fixture_allowlist(extra: str = "") -> api_drift.Allowlist:
    import tomllib

    return api_drift.parse_allowlist(tomllib.loads(ALLOWLIST.read_text() + "\n" + extra))


def fixture_report(extra_allowlist: str = "") -> api_drift.Report:
    allowlist = load_fixture_allowlist(extra_allowlist)
    scan = api_drift.scan_crate(CRATE, allowlist)
    docs = api_drift.extract_doc_routes(DOCS, allowlist.settings)
    events = api_drift.extract_doc_events_from_text(
        (DOCS / allowlist.settings["docs_events_page"]).read_text()
    )
    return api_drift.compare(docs, events, scan, allowlist, "docs123", "crate456")


def run_cli(*args: str) -> tuple[int, str, str]:
    out, err = io.StringIO(), io.StringIO()
    with contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
        code = api_drift.run(list(args))
    return code, out.getvalue(), err.getvalue()


class NormalizationTests(unittest.TestCase):
    def test_docs_template_strips_links_backslashes_and_param_names(self):
        raw = "/channels/[\\{channel.id\\}](/developers/resources/channel#channel-object)/messages/[\\{message.id\\}](/x)"
        self.assertEqual(api_drift.normalize_template(raw), "/channels/{}/messages/{}")

    def test_docs_template_handles_changelog_brackets_and_queries(self):
        self.assertEqual(api_drift.normalize_template("/guilds/[\\{guild.id\\}]/pins"), "/guilds/{}/pins")
        self.assertEqual(api_drift.normalize_template(" /a/\\{b\\_id\\}?x=1 "), "/a/{}")
        self.assertEqual(api_drift.normalize_template("/users/@me/"), "/users/@me")

    def test_event_names_follow_the_docs_rule(self):
        self.assertEqual(api_drift.event_name_from_title("Guild Members Chunk"), "GUILD_MEMBERS_CHUNK")
        self.assertEqual(api_drift.event_name_from_title(" Voice Channel Effect Send "), "VOICE_CHANNEL_EFFECT_SEND")


class DocsExtractionTests(unittest.TestCase):
    def setUp(self):
        self.settings = dict(api_drift.DEFAULT_SETTINGS)
        self.routes = api_drift.extract_doc_routes(DOCS, self.settings)

    def test_route_set(self):
        self.assertEqual(
            set(self.routes),
            {
                ("GET", "/guilds/{}/widget"),
                ("PATCH", "/guilds/{}/widget"),
                ("DELETE", "/guilds/{}/widget"),
                ("GET", "/channels/{}/messages/{}"),
                ("POST", "/webhooks/{}/{}"),
                ("GET", "/channels/{}/pins"),
                ("GET", "/sparkles/{}"),
                ("GET", "/secret-widgets"),
            },
        )

    def test_changelog_routes_are_excluded(self):
        self.assertNotIn(("POST", "/guilds/{}/legacy-widget"), self.routes)

    def test_duplicates_merge_and_keep_locations(self):
        locations = self.routes[("GET", "/guilds/{}/widget")]
        self.assertEqual(
            sorted(route.file for route in locations),
            ["developers/resources/widget.mdx", "developers/topics/duplicate.mdx"],
        )
        first = next(route for route in locations if route.file.endswith("widget.mdx"))
        self.assertEqual(first.heading, "Get Widget")
        self.assertEqual(first.line, 8)

    def test_deprecated_heading_is_flagged(self):
        self.assertTrue(self.routes[("GET", "/channels/{}/pins")][0].deprecated)
        self.assertFalse(self.routes[("GET", "/sparkles/{}")][0].deprecated)

    def test_events_come_from_the_receive_table_and_headings(self):
        text = (DOCS / "developers/events/gateway-events.mdx").read_text()
        events = api_drift.extract_doc_events_from_text(text)
        self.assertEqual(
            set(events),
            {"HELLO", "READY", "MESSAGE_CREATE", "GUILD_MEMBERS_CHUNK", "VOICE_CHANNEL_EFFECT_SEND", "SHINY_NEW_EVENT"},
        )
        self.assertEqual(events["SHINY_NEW_EVENT"].sources, {"heading"})
        self.assertEqual(events["READY"].sources, {"table", "heading"})

    def test_events_section_missing_returns_nothing(self):
        self.assertEqual(api_drift.extract_doc_events_from_text("## Send Events\n| [Identify](#x) | y |\n"), {})


class RustScanningTests(unittest.TestCase):
    def test_mask_blanks_comments_and_string_contents(self):
        source = 'let a = "x{y}"; // c { \n/* d { /* nested } */ */ let b = \'{\'; let c: &\'static str = r#"q}"#;'
        masked = api_drift.mask_rust(source)
        self.assertEqual(len(masked), len(source))
        self.assertEqual(masked.count("{"), 0)
        self.assertEqual(masked.count("}"), 0)
        self.assertIn("'static", masked)
        self.assertIn("\n", masked)

    def test_strip_test_code_removes_test_modules_and_functions(self):
        source = (
            "struct A {\n    #[cfg(test)]\n    field: u8,\n}\n"
            "#[cfg(test)]\nmod tests;\n"
            "#[cfg(test)]\nfn helper() { let s = \"}\"; call(\"/hidden\"); }\n"
            "#[cfg(test)]\nmod inline {\n    fn t() { \"/also-hidden\"; }\n}\n"
            "fn kept() { \"/kept\"; }\n"
        )
        original, _ = api_drift.strip_test_code(source)
        self.assertNotIn("/hidden", original)
        self.assertNotIn("/also-hidden", original)
        self.assertIn("/kept", original)
        self.assertIn("field: u8", original)
        self.assertEqual(original.count("\n"), source.count("\n"))

    def test_manifest_rows(self):
        rows, errors = api_drift.parse_manifest_text((CRATE / "src/http/routes.rs").read_text(), "routes.rs")
        self.assertEqual(errors, [])
        self.assertEqual(
            {(row.method, row.path) for row in rows},
            {
                ("GET", "/guilds/{}/widget"),
                ("PATCH", "/guilds/{}/widget"),
                ("GET", "/channels/{}/messages/{}"),
                ("GET", "/channels/{}/pins"),
                ("GET", "/sparkles/{}"),
                ("POST", "/webhooks/{}/{}"),
                ("POST", "/legacy/thing"),
                ("POST", "/oauth2/token"),
            },
        )
        pins = next(row for row in rows if row.id == "GetPinnedMessages")
        self.assertEqual(pins.template, "/channels/{channel.id}/pins")
        self.assertEqual(pins.query, ["limit"])
        self.assertEqual(pins.api, ["get_pins", "get_pins_limited"])
        self.assertEqual(pins.was, [("pins", "get_pins")])
        self.assertEqual((pins.auth, pins.body, pins.response), ("bot", "()", "Value"))
        oauth = next(row for row in rows if row.id == "OAuth2Token")
        self.assertEqual((oauth.client, oauth.api), ("oauth2", ["exchange_code"]))

    def test_manifest_rejects_lines_it_does_not_understand(self):
        text = (
            "route_manifest! {\n"
            '    Good: GET "/a" bot, query [], body () => (), api [a];\n'
            '    Bad: FETCH "/b" bot, query [], body () => (), api [b];\n'
            '    Wrapped: GET "/c" bot,\n'
            "}\n"
        )
        rows, errors = api_drift.parse_manifest_text(text, "routes.rs")
        self.assertEqual([row.id for row in rows], ["Good"])
        self.assertEqual(len(errors), 2)
        self.assertIn("routes.rs:3: not a manifest row", errors[0])
        _, errors = api_drift.parse_manifest_text("fn main() {}", "routes.rs")
        self.assertIn("no `route_manifest! { ... }` invocation", errors[0])

    def test_functions_record_owner_visibility_and_deprecation(self):
        functions = {
            function.name: function
            for function in api_drift.extract_functions_from_text((CRATE / "src/http.rs").read_text(), "src/http.rs")
        }
        self.assertTrue(functions["get_widget"].endpoint)
        self.assertEqual(functions["get_widget"].owner, "RestClient")
        self.assertFalse(functions["is_replay_safe"].endpoint)
        self.assertFalse(functions["execute_webhook_path"].endpoint)
        self.assertTrue(functions["pins"].deprecated)
        self.assertNotIn("test_only_endpoint", functions)
        # Comments and string literals do not count as route references.
        self.assertNotIn("GetChannelMessage", functions["get_sparkle"].body)
        self.assertNotIn("LegacyThing", functions["get_sparkle"].body)

    def test_decoder_event_names(self):
        names = api_drift.extract_event_names_from_text(
            (CRATE / "src/event/decode.rs").read_text(), api_drift.DEFAULT_SETTINGS["event_name_pattern"]
        )
        self.assertEqual(set(names), {"READY", "MESSAGE_CREATE", "MESSAGE_UPDATE", "GUILD_MEMBERS_CHUNK"})

    def test_exclude_globs(self):
        patterns = ["src/http/**/tests.rs", "developers/change-log/"]
        self.assertTrue(api_drift._is_excluded("src/http/tests.rs", patterns))
        self.assertTrue(api_drift._is_excluded("src/http/guild/tests.rs", patterns))
        self.assertTrue(api_drift._is_excluded("developers/change-log/2026.mdx", patterns))
        self.assertFalse(api_drift._is_excluded("src/http/guild.rs", patterns))

    def test_scan_skips_excluded_test_file(self):
        scan = api_drift.scan_crate(CRATE, load_fixture_allowlist())
        self.assertNotIn("harness_only", {function.name for function in scan.functions})


class AllowlistTests(unittest.TestCase):
    def test_reason_is_mandatory(self):
        with self.assertRaisesRegex(api_drift.ConfigError, "reason"):
            api_drift.parse_allowlist({"ignored_route": [{"method": "GET", "path": "/x", "reason": "short"}]})

    def test_helper_route_table_is_gone(self):
        with self.assertRaisesRegex(api_drift.ConfigError, "unknown allowlist tables"):
            api_drift.parse_allowlist(
                {"helper_route": [{"method": "GET", "path": "/x", "reason": "a long enough reason"}]}
            )

    def test_duplicates_across_tables_are_rejected(self):
        entry = {"method": "GET", "path": "/x/{a}", "reason": "a long enough reason"}
        other = dict(entry, path="/x/{b}")
        with self.assertRaisesRegex(api_drift.ConfigError, "already listed"):
            api_drift.parse_allowlist({"ignored_route": [entry], "crate_only_route": [other]})

    def test_unknown_keys_tables_settings_and_bad_values(self):
        cases = [
            ({"ignored_route": [{"method": "GET", "path": "/x", "reason": "a long enough reason", "note": 1}]}, "unknown keys"),
            ({"ignored_routes": []}, "unknown allowlist tables"),
            ({"settings": {"min_routes": 1}}, "unknown settings"),
            ({"ignored_route": [{"method": "FETCH", "path": "/x", "reason": "a long enough reason"}]}, "method"),
            ({"ignored_route": [{"method": "GET", "path": "x", "reason": "a long enough reason"}]}, "path"),
            ({"ignored_event": [{"name": "lower_case", "reason": "a long enough reason"}]}, "UPPER_SNAKE"),
            ({"ignored_event": [{"name": "X", "functions": ["f"], "reason": "a long enough reason"}]}, "functions"),
        ]
        for data, message in cases:
            with self.subTest(message=message), self.assertRaisesRegex(api_drift.ConfigError, message):
                api_drift.parse_allowlist(data)

    def test_real_allowlist_parses(self):
        allowlist = api_drift.load_allowlist(CI_DIR / "api-drift-allowlist.toml")
        self.assertGreater(len(allowlist.keys("crate_only_route")), 0)


class CompareTests(unittest.TestCase):
    def test_fixture_report(self):
        report = fixture_report()
        self.assertEqual(report.missing_routes, [("DELETE", "/guilds/{}/widget")])
        self.assertEqual(report.extra_routes, [("POST", "/legacy/thing")])
        self.assertNotIn(("GET", "/secret-widgets"), report.missing_routes)
        self.assertEqual(report.missing_events, ["SHINY_NEW_EVENT", "VOICE_CHANNEL_EFFECT_SEND"])
        self.assertEqual(report.extra_events, ["MESSAGE_UPDATE"])
        self.assertEqual(report.maintenance, [])
        self.assertEqual(report.heading_only_events, ["SHINY_NEW_EVENT"])
        self.assertTrue(report.has_findings)

    def test_fingerprint_ignores_commits_and_time(self):
        first = fixture_report()
        second = fixture_report()
        second.docs_commit, second.generated_at = "other", "later"
        self.assertEqual(first.fingerprint, second.fingerprint)

    def test_allowlisting_everything_clears_the_report(self):
        report = fixture_report(
            """
[[ignored_route]]
method = "DELETE"
path = "/guilds/{guild.id}/widget"
reason = "Fixture: not wrapped on purpose."

[[crate_only_route]]
method = "POST"
path = "/legacy/thing"
reason = "Fixture: kept for compatibility."

[[ignored_event]]
name = "SHINY_NEW_EVENT"
reason = "Fixture: not decoded on purpose."

[[ignored_event]]
name = "VOICE_CHANNEL_EFFECT_SEND"
reason = "Fixture: not decoded on purpose."

[[crate_only_event]]
name = "MESSAGE_UPDATE"
reason = "Fixture: undocumented but real."
"""
        )
        self.assertFalse(report.has_findings, report.maintenance)

    def test_stale_entries_are_reported(self):
        report = fixture_report(
            """
[[ignored_route]]
method = "GET"
path = "/never-documented"
reason = "Fixture: ignored route Discord no longer documents."

[[ignored_route]]
method = "GET"
path = "/sparkles/{id}"
reason = "Fixture: ignored route the crate implements now."

[[crate_only_route]]
method = "GET"
path = "/guilds/{id}/widget"
reason = "Fixture: crate-only entry Discord now documents."

[[crate_only_route]]
method = "GET"
path = "/gone"
reason = "Fixture: crate-only entry the crate no longer sends."
"""
        )
        text = "\n".join(report.maintenance)
        self.assertIn("/never-documented: no longer documented", text)
        self.assertIn("GET /sparkles/{}: the crate implements it now", text)
        self.assertIn("Discord documents it now", text)
        self.assertIn("GET /gone: no longer found in the crate", text)

    def test_evidence_in_comments_does_not_count(self):
        allowlist = load_fixture_allowlist(
            """
[[crate_only_route]]
method = "POST"
path = "/legacy/thing"
evidence = ['"/only/in/a/comment"']
reason = "Fixture: evidence that exists only inside a comment."
"""
        )
        scan = api_drift.scan_crate(CRATE, allowlist)
        self.assertEqual(
            api_drift.missing_evidence(allowlist, scan.evidence_corpus),
            ['[[crate_only_route]] POST /legacy/thing: evidence `"/only/in/a/comment"` no longer appears in src/'],
        )


def binding_notes(**edits: tuple[str, str]) -> list[str]:
    """Manifest binding notes for a copy of the fixture crate with edits.

    Each keyword names a fixture file (``http``, ``routes``, ``oauth2``,
    ``goldens``) and
    maps to an ``(old, new)`` replacement applied to it.
    """

    files = {
        "http": "src/http.rs",
        "routes": "src/http/routes.rs",
        "oauth2": "src/oauth2.rs",
        "goldens": "src/http/tests/goldens.rs",
    }
    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp) / "crate"
        shutil.copytree(CRATE, root)
        for key, (old, new) in edits.items():
            path = root / files[key]
            text = path.read_text()
            if old not in text:
                raise AssertionError(f"fixture edit does not apply: {old!r}")
            path.write_text(text.replace(old, new))
        allowlist = load_fixture_allowlist()
        return api_drift.manifest_binding(allowlist, api_drift.scan_crate(root, allowlist))


class ManifestBindingTests(unittest.TestCase):
    def test_fixture_is_bound(self):
        self.assertEqual(binding_notes(), [])

    def test_listed_method_must_exist(self):
        notes = binding_notes(routes=("api [get_sparkle]", "api [get_sparkle, get_sparkles]"))
        self.assertEqual(notes, ["route manifest: `GetSparkle` lists `RestClient::get_sparkles`, which does not exist"])

    def test_listed_method_must_send_its_route(self):
        notes = binding_notes(http=("RouteId::GetSparkle", "RouteId::GetChannelMessage"))
        self.assertIn(
            "route manifest: `RestClient::get_sparkle` is listed under `GetSparkle` but never sends "
            "`RouteId::GetSparkle` (directly or through a `self.` call)",
            notes,
        )

    def test_unlisted_endpoint_is_reported(self):
        notes = binding_notes(routes=("api [get_pins, get_pins_limited]", "api [get_pins]"))
        self.assertEqual(len(notes), 1)
        self.assertIn("endpoint `RestClient::get_pins_limited` (src/http.rs:", notes[0])

    def test_removed_name_must_be_gone_or_deprecated(self):
        notes = binding_notes(http=('#[deprecated(note = "use get_pins")]\n', ""))
        self.assertEqual(len(notes), 2, notes)
        self.assertIn("`RestClient::pins` is recorded as removed (`was`) under `GetPinnedMessages`", notes[0])
        # Its golden comparison no longer compares a deprecated forward.
        self.assertIn("is not a deprecated forward of the manifest", notes[1])
        notes = binding_notes(routes=("was [pins => get_pins]", "was [pins => get_pin]"))
        self.assertIn("route manifest: `GetPinnedMessages` maps `pins` to `get_pin`, which the row does not list", notes)

    def test_deprecated_forward_must_call_its_replacement(self):
        notes = binding_notes(http=("        self.get_pins(channel_id).await\n    }\n\n    pub async fn get_sparkle",
                                    "        self.get_pins_limited(channel_id, None).await\n    }\n\n    pub async fn get_sparkle"))
        self.assertEqual(len(notes), 1, notes)
        self.assertIn("deprecated `RestClient::pins`", notes[0])
        self.assertIn("does not call its replacement `get_pins`", notes[0])

    def test_every_deprecated_forward_has_a_golden_case(self):
        notes = binding_notes(goldens=("vec![forward!(GetPinnedMessages, pins => get_pins(1))]", "vec![]"))
        self.assertEqual(
            notes,
            [
                "deprecated forward `pins` (`GetPinnedMessages`) has no "
                "`forward!(GetPinnedMessages, pins => get_pins(..))` golden case in src/http/tests/goldens.rs"
            ],
        )
        notes = binding_notes(
            goldens=("vec![forward!(", "vec![forward!(GetSparkle, sparkle => get_sparkle(\"x\")), forward!(")
        )
        self.assertEqual(
            notes,
            [
                "src/http/tests/goldens.rs: `forward!(GetSparkle, sparkle => get_sparkle(..))` is not a "
                "deprecated forward of the manifest"
            ],
        )

    def test_direct_methods_outside_the_transport_are_reported(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp) / "crate"
            shutil.copytree(CRATE, root)
            (root / "src/http/extra.rs").write_text(
                "impl RestClient {\n    fn sneaky(&self) { self.raw(Method::POST, \"/x\"); }\n}\n"
            )
            allowlist = load_fixture_allowlist()
            notes = api_drift.manifest_binding(allowlist, api_drift.scan_crate(root, allowlist))
        self.assertEqual(
            notes, ["src/http/extra.rs:2: Method::POST: endpoints name their route with `RouteId`, not an HTTP method"]
        )

    def test_duplicate_ids_and_unparseable_rows_are_reported(self):
        row = '    GetSparkle: GET "/sparkles/{sparkle.id}" bot, query [], body () => Value, api [get_sparkle];\n'
        notes = binding_notes(routes=(row, row + row + "    Broken: GET\n"))
        self.assertTrue(any("not a manifest row" in note for note in notes), notes)
        self.assertTrue(any("`GetSparkle` is listed twice" in note for note in notes), notes)


class RenderTests(unittest.TestCase):
    def test_markdown_has_marker_sections_and_fingerprint(self):
        report = fixture_report()
        text = api_drift.render_markdown(report)
        self.assertTrue(text.startswith(api_drift.REPORT_MARKER))
        self.assertIn("**Drift found.**", text)
        self.assertIn("`DELETE /guilds/{}/widget` - Delete Widget (`developers/resources/widget.mdx:", text)
        self.assertIn("`POST /legacy/thing` - `LegacyThing` (`src/http/routes.rs:", text)
        self.assertIn("`SHINY_NEW_EVENT`", text)
        self.assertIn(f"<!-- api-drift-fingerprint: {report.fingerprint} -->", text)

    def test_markdown_truncates_but_keeps_fingerprint(self):
        report = fixture_report()
        text = api_drift.render_markdown(report, max_chars=600)
        self.assertLessEqual(len(text), 600)
        self.assertIn("truncated", text)
        self.assertIn(f"<!-- api-drift-fingerprint: {report.fingerprint} -->", text)

    def test_json_round_trips(self):
        data = json.loads(api_drift.render_json(fixture_report()))
        self.assertEqual(data["missing_routes"], [{"method": "DELETE", "path": "/guilds/{}/widget"}])
        self.assertEqual(data["docs_commit"], "docs123")
        self.assertTrue(data["has_findings"])


class CliTests(unittest.TestCase):
    base = ("--repo", str(CRATE), "--allowlist", str(ALLOWLIST))

    def test_drift_exits_zero_unless_asked(self):
        code, out, _ = run_cli(*self.base, "--docs", str(DOCS))
        self.assertEqual(code, api_drift.EXIT_OK)
        self.assertIn("Drift found", out)
        code, _, _ = run_cli(*self.base, "--docs", str(DOCS), "--fail-on-drift")
        self.assertEqual(code, api_drift.EXIT_DRIFT)

    def test_outputs_are_written(self):
        with tempfile.TemporaryDirectory() as tmp:
            md, js = Path(tmp) / "r.md", Path(tmp) / "r.json"
            code, _, _ = run_cli(*self.base, "--docs", str(DOCS), "--output", str(md), "--json-output", str(js))
            self.assertEqual(code, 0)
            self.assertIn(api_drift.REPORT_MARKER, md.read_text())
            self.assertTrue(json.loads(js.read_text())["has_findings"])

    def test_self_check_passes_on_fixture(self):
        code, out, err = run_cli(*self.base, "--self-check")
        self.assertEqual(code, api_drift.EXIT_OK, err)
        self.assertIn("allowlist and route manifest match the crate", out)

    def test_broken_extraction_exits_two(self):
        with tempfile.TemporaryDirectory() as tmp:
            allowlist = Path(tmp) / "allow.toml"
            allowlist.write_text(ALLOWLIST.read_text().replace("min_docs_routes = 1", "min_docs_routes = 500"))
            code, _, err = run_cli("--repo", str(CRATE), "--allowlist", str(allowlist), "--docs", str(DOCS))
            self.assertEqual(code, api_drift.EXIT_BROKEN)
            self.assertIn("extraction looks broken", err)

    def test_missing_docs_checkout_exits_two(self):
        with tempfile.TemporaryDirectory() as tmp:
            code, _, err = run_cli(*self.base, "--docs", tmp)
            self.assertEqual(code, api_drift.EXIT_BROKEN)
            self.assertIn("docs route root not found", err)


class IssuePlanTests(unittest.TestCase):
    def report(self, findings=True, fingerprint="abc"):
        return {
            "has_findings": findings,
            "fingerprint": fingerprint,
            "docs_commit": "d1",
            "crate_commit": "c1",
            "missing_routes": [{"method": "GET", "path": "/x"}] if findings else [],
            "extra_routes": [],
            "missing_events": [],
            "extra_events": [],
            "maintenance": [],
        }

    @staticmethod
    def managed(number, fingerprint="abc"):
        return {
            "number": number,
            "body": f"{drift_issue.REPORT_MARKER}\nreport\n<!-- api-drift-fingerprint: {fingerprint} -->",
        }

    def kinds(self, actions):
        return [(action.kind, action.number) for action in actions]

    def test_creates_when_drift_and_no_managed_issue(self):
        human = {"number": 3, "body": "I labelled this api-drift by hand"}
        actions = drift_issue.plan(self.report(), "body", [human])
        self.assertEqual(self.kinds(actions), [("create", None)])
        self.assertEqual(actions[0].title, "Discord API drift: 1 missing routes")

    def test_unchanged_drift_edits_without_comment(self):
        actions = drift_issue.plan(self.report(), "body", [self.managed(5)])
        self.assertEqual(self.kinds(actions), [("edit", 5)])

    def test_changed_drift_edits_and_comments(self):
        actions = drift_issue.plan(self.report(fingerprint="new"), "body", [self.managed(5)])
        self.assertEqual(self.kinds(actions), [("edit", 5), ("comment", 5)])

    def test_duplicates_are_closed_and_oldest_kept(self):
        actions = drift_issue.plan(self.report(), "body", [self.managed(9), self.managed(4)])
        self.assertEqual(self.kinds(actions), [("close", 9), ("edit", 4)])

    def test_clean_report_closes_managed_issue_only(self):
        human = {"number": 2, "body": "unrelated"}
        actions = drift_issue.plan(self.report(findings=False), "body", [human, self.managed(7)])
        self.assertEqual(self.kinds(actions), [("close", 7)])

    def test_clean_report_without_issue_does_nothing(self):
        self.assertEqual(drift_issue.plan(self.report(findings=False), "body", []), [])

    def test_dry_run_reads_issue_file(self):
        with tempfile.TemporaryDirectory() as tmp:
            report = Path(tmp) / "r.json"
            body = Path(tmp) / "r.md"
            issues = Path(tmp) / "issues.json"
            report.write_text(json.dumps(self.report(fingerprint="new")))
            body.write_text("body")
            issues.write_text(json.dumps([self.managed(5)]))
            out = io.StringIO()
            with contextlib.redirect_stdout(out):
                code = drift_issue.run([str(report), str(body), "--dry-run", "--issues-json", str(issues)])
            self.assertEqual(code, 0)
            self.assertEqual(out.getvalue().splitlines(), ["edit: #5 - Discord API drift: 1 missing routes", "comment: #5"])


if __name__ == "__main__":
    unittest.main()
