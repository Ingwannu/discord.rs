#!/usr/bin/env python3
"""Regenerate src/model/doc_examples/payloads.rs from a discord-api-docs checkout.

    python3 ci/doc_examples.py --docs <discord-api-docs clone> [--out <file>]

The file holds Discord's documented example payloads as raw-string constants
for the model round-trip tests in src/model/doc_examples.rs. Strict JSON
examples are copied verbatim (one has a trailing comma removed). Component
examples are `jsonc`: comments, `...spread` placeholder lines, and trailing
commas are stripped and every component is re-emitted as compact JSON.
Standard library only. The committed file was generated from docs commit
3714862; regenerate when bumping the docs revision the crate targets and
review the diff like any other test fixture change.

[Decision Log]
- 목적과 의도: 판별값을 바꾼 모델이 Discord 문서의 예시 payload를 여전히 디코딩하고 같은
  JSON으로 다시 쓰는지 테스트로 고정한다(3.0 시리즈 3).
- 기존 구현 및 제약 조건: 단위 테스트는 손으로 쓴 최소 JSON만 썼다. 문서 예시는 MDX 안의
  코드 블록이고, 컴포넌트 예시는 주석·생략 표시가 있는 jsonc다.
- 검토한 주요 대안: (1) 테스트에서 문서 체크아웃을 읽기, (2) 예시를 손으로 복사,
  (3) 스크립트로 추출해 상수 파일을 커밋.
- 선택한 방식: (3).
- 다른 대안 대신 이 방식을 선택한 이유: (1)은 cargo test가 네트워크·외부 경로에 의존한다.
  (2)는 출처와 차이를 재현할 수 없다. (3)은 테스트가 자급적이고 문서 갱신 때 diff로 검토된다.
- 장점, 단점 및 영향: 문서 개정마다 재생성이 필요하다(제목이 바뀌면 이 스크립트도 고친다).
"""
import argparse
import json
import re
import sys
from pathlib import Path

PARSER = argparse.ArgumentParser(description=__doc__.split("\n")[0])
PARSER.add_argument("--docs", type=Path, required=True, help="discord-api-docs checkout")
PARSER.add_argument(
    "--out",
    type=Path,
    default=Path(__file__).resolve().parent.parent / "src/model/doc_examples/payloads.rs",
)
ARGS = PARSER.parse_args()
DOCS = ARGS.docs / "developers"
OUT = ARGS.out


def section(path, heading):
    s = (DOCS / path).read_text()
    i = s.index(heading)
    m = re.compile(r"```json[^\n]*\n(.*?)\n```", re.S).search(s, i)
    return m.group(1)


def strip_comments(text):
    """Drops `// ...` comments outside JSON strings (URLs keep their `//`)."""
    out, i, in_string = [], 0, False
    while i < len(text):
        ch = text[i]
        if in_string:
            out.append(ch)
            if ch == "\\":
                out.append(text[i + 1])
                i += 2
                continue
            if ch == '"':
                in_string = False
        elif ch == '"':
            in_string = True
            out.append(ch)
        elif text.startswith("//", i):
            while i < len(text) and text[i] != "\n":
                i += 1
            continue
        else:
            out.append(ch)
        i += 1
    return "".join(out)


def strip_trailing_commas(text):
    return re.sub(r",(\s*[}\]])", r"\1", text)


# (const name, model, file, heading)
EXAMPLES = [
    ("GUILD_TEXT_CHANNEL", "resources/channel.mdx", "###### Example Guild Text Channel"),
    ("GUILD_ANNOUNCEMENT_CHANNEL", "resources/channel.mdx", "###### Example Guild Announcement Channel"),
    ("GUILD_VOICE_CHANNEL", "resources/channel.mdx", "###### Example Guild Voice Channel"),
    ("DM_CHANNEL", "resources/channel.mdx", "###### Example DM Channel"),
    ("GROUP_DM_CHANNEL", "resources/channel.mdx", "###### Example Group DM Channel"),
    ("CHANNEL_CATEGORY", "resources/channel.mdx", "###### Example Channel Category"),
    ("THREAD_CHANNEL", "resources/channel.mdx", "###### Example Thread Channel"),
    ("GUILD", "resources/guild.mdx", "###### Example Guild\n"),
    ("GUILD_ONBOARDING", "resources/guild.mdx", "###### Example Guild Onboarding"),
    ("PARTIAL_INTEGRATION", "resources/audit-log.mdx", "###### Example Partial Integration Object"),
    ("MESSAGE", "resources/message.mdx", "###### Example Message"),
    ("CROSSPOSTED_MESSAGE", "resources/message.mdx", "###### Example Crossposted Message"),
    ("SHARED_CLIENT_THEME", "resources/message.mdx", "###### Example Shared Client Theme"),
    ("INVITE", "resources/invite.mdx", "###### Example Invite Object"),
    ("INCOMING_WEBHOOK", "resources/webhook.mdx", "###### Example Incoming Webhook"),
    ("CHANNEL_FOLLOWER_WEBHOOK", "resources/webhook.mdx", "###### Example Channel Follower Webhook"),
    ("APPLICATION_WEBHOOK", "resources/webhook.mdx", "###### Example Application Webhook"),
    ("STICKER", "resources/sticker.mdx", "###### Example Sticker"),
    ("STICKER_PACK", "resources/sticker.mdx", "###### Example Sticker Pack"),
    ("SKU", "resources/sku.mdx", "SKU Example"),
    ("ENTITLEMENT", "resources/entitlement.mdx", "Entitlement Example"),
    ("SUBSCRIPTION", "resources/subscription.mdx", "Subscription Example"),
    ("USER", "resources/user.mdx", "###### Example User"),
    ("PARTIAL_GUILD", "resources/user.mdx", "###### Example Partial Guild"),
    ("STAGE_INSTANCE", "resources/stage-instance.mdx", "###### Example Stage Instance"),
    ("APPLICATION", "resources/application.mdx", "###### Example Application Object"),
    ("SLASH_COMMAND", "interactions/application-commands.mdx", "Example Slash Command"),
    ("AUTO_MODERATION_RULE", "resources/auto-moderation.mdx", "###### Example Auto Moderation Rule"),
    ("GATEWAY_PRESENCE_UPDATE", "events/gateway-events.mdx", "###### Example Gateway Presence Update"),
    ("ACTIVITY", "events/gateway-events.mdx", "###### Example Activity\n"),
    ("ACTIVITY_WITH_RICH_PRESENCE", "events/gateway-events.mdx", "###### Example Activity with Rich Presence"),
]

INTERACTIONS = []
s = (DOCS / "interactions/application-commands.mdx").read_text()
for i, m in enumerate(re.finditer(r"Example Interaction[^\n]*\n+(?:[^\n`]*\n+)*?```json[^\n]*\n(.*?)\n```", s, re.S)):
    INTERACTIONS.append((f"COMMAND_INTERACTION_{i + 1}", m.group(1)))

out = []
out.append("//! Discord's documented example payloads (discord-api-docs `3714862`),")
out.append("//! generated by `ci/doc_examples.py`; see `super` for how they are used.")
out.append("//! Copied verbatim except: the guild voice channel example's trailing")
out.append("//! comma is removed (it is not valid JSON), and component examples (`jsonc`")
out.append("//! blocks) have their comments, `...spread` placeholder lines, and trailing")
out.append("//! commas stripped and each component re-emitted compactly.")
out.append("")

for name, path, heading in EXAMPLES:
    body = section(path, heading)
    if name == "GUILD_VOICE_CHANNEL":
        body = strip_trailing_commas(body)
    json.loads(body)
    out.append(f"/// {path} — {heading.strip('# ').strip()}")
    out.append(f'pub(super) const {name}: &str = r##"{body}"##;')
    out.append("")

for name, body in INTERACTIONS:
    json.loads(body)
    out.append("/// interactions/application-commands.mdx — Example Interaction")
    out.append(f'pub(super) const {name}: &str = r##"{body}"##;')
    out.append("")

# components
s = (DOCS / "components/reference.mdx").read_text()
components = []
current = None
for m in re.finditer(r"(?m)^## ([^\n]+)$|```jsonc\n(.*?)\n\s*```", s, re.S):
    if m.group(1):
        current = m.group(1).strip()
        continue
    text = m.group(2)
    text = strip_comments(text)
    text = "\n".join(l for l in text.split("\n") if not l.strip().startswith("..."))
    text = strip_trailing_commas(text)
    try:
        value = json.loads(text)
    except json.JSONDecodeError as error:
        print("skip", current, error, file=sys.stderr)
        continue
    roots = []
    if isinstance(value.get("components"), list):
        roots = value["components"]
    data = value.get("data")
    if isinstance(data, dict) and isinstance(data.get("components"), list):
        roots = data["components"]
    for component in roots:
        components.append((current, json.dumps(component, separators=(",", ":"))))

out.append("/// components/reference.mdx — every component of every message, modal,")
out.append("/// and interaction example, as `(section, component JSON)`.")
out.append("pub(super) const COMPONENTS: &[(&str, &str)] = &[")
for section_name, body in components:
    out.append(f'    ("{section_name}", r##"{body}"##),')
out.append("];")
out.append("")

OUT.write_text("\n".join(out))
print(f"{len(EXAMPLES)} examples, {len(INTERACTIONS)} interactions, {len(components)} components", file=sys.stderr)
