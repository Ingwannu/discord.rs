# discord.rs ����

`discordrs` ũ����Ʈ�� Ÿ�� ������ Discord ���� ����� ���� ���� ����Ʈ�Դϴ�.

> discord.rs�� `Client`, `RestClient`, Ÿ�� ��/�̺�Ʈ/���ͷ���, Ŀ�ǵ� ���, Components V2 ����, ������ cache/collector ���̾�, voice receive ������ �߽����� �����˴ϴ�.

�귣�� �̸��� discord.rs�̰�, crates.io ��Ű����� Rust import ��δ� ��� `discordrs`�� ����մϴ�.

## ���� �� ����

- [���� ����](#/ko/guide/getting-started)
- [��Ű��ó](#/ko/guide/architecture)
- [��� ���̵�](#/ko/guide/usage-guide)
- [��� API](#/ko/api/builders)
- [����Ʈ���� API](#/ko/api/gateway)
- [HTTP �� ���� API](#/ko/api/http-and-helpers)

## �ٽ� ��Ÿ�� ǥ��

- `Client`: `EventHandler::handle_event(...)` ����� Ÿ�� ����Ʈ���� ��Ÿ��
- `RestClient`: ���� rate-limit ���¸� �����ϴ� ������ REST Ŭ���̾�Ʈ
- `parse_interaction(...)`: Ÿ�� ���ͷ��� ���ڵ� ������
- `SlashCommandBuilder` / `UserCommandBuilder` / `MessageCommandBuilder`
- `CacheHandle` �� ���� manager Ÿ��
- `collectors` ��� �÷��� �ڿ� �ִ� collector Ÿ�Ե�
- `connect_voice_runtime(...)`, `VoiceOpusDecoder`, ������ DAVE hook
- Poll, Subscription, Entitlement, Soundboard, Thread, Forum, Invite, Integration ���� Ÿ�� REST/Event ǥ��

## ��� �÷���

```toml
[dependencies]
# �ھ ���
discordrs = "1.2.2"

# Ÿ�� ����Ʈ���� ��Ÿ��
discordrs = { version = "1.2.2", features = ["gateway"] }

# ĳ�� �Ǵ� �÷��Ͱ� ���Ե� ����Ʈ���� ��Ÿ��
discordrs = { version = "1.2.2", features = ["gateway", "cache"] }
discordrs = { version = "1.2.2", features = ["gateway", "collectors"] }

# ���ͷ��� ��������Ʈ
discordrs = { version = "1.2.2", features = ["interactions"] }

# Voice receive / Opus decode
discordrs = { version = "1.2.2", features = ["voice"] }

# ������ DAVE/MLS hook
discordrs = { version = "1.2.2", features = ["voice", "dave"] }
```

## ��� ��ȯ

������ �Ʒ� `LANG` ��ư���� �� ��ȯ�� �� �ֽ��ϴ�.
