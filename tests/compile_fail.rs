//! Compile-fail contract tests (trybuild) for the phase 3 primitives: typed
//! IDs, open discriminants, and patch fields must turn misuse into compile
//! errors (docs/design/3.0-types-events-requests.md §5 "필수 테스트"),
//! including the endpoint and polymorphic-target cases of series 2 step B
//! (`channel_id_for_get_guild.rs`, `user_id_as_role_overwrite.rs`) and the
//! discriminant cases of series 3 (`u8_for_channel_type.rs`,
//! `discriminant_default.rs`, `discriminant_storage_is_private.rs`). The public
//! async traits' `Send` contract is pinned the same way: an `async fn` impl
//! whose future is not `Send` must fail at the impl
//! (`cache_backend_future_not_send.rs`; `CacheBackend` is the async trait
//! available with default features, which this test runs with).
//!
//! The expected `.stderr` files in `tests/ui/` are rustc diagnostics, which
//! change wording between Rust releases. They are therefore kept for one
//! toolchain only, the pinned CI toolchain (`RUST_TOOLCHAIN` in
//! `.github/workflows/ci.yml`, currently 1.99.0), and this test does nothing
//! unless `DISCORDRS_TRYBUILD=1` is set:
//!
//! ```text
//! DISCORDRS_TRYBUILD=1 cargo +1.99.0 test --locked --test compile_fail
//! ```
//!
//! CI runs it in the `rust-check` job. After bumping the pinned toolchain,
//! regenerate and review the snapshots (details in
//! `discordrsdocs/docs/guide/testing-and-coverage.md`):
//!
//! ```text
//! DISCORDRS_TRYBUILD=1 TRYBUILD=overwrite cargo +<new> test --locked --test compile_fail
//! ```
//!
//! ```text
//! [Decision Log]
//! - 목적과 의도: 채널 ID를 길드 자리(엔드포인트 포함)에 넘기기, 사용자 ID로 역할 덮어쓰기
//!   만들기, u64 암시 변환, ID Default, 비-null 패치에 null 같은 오용이 계속 컴파일 오류로
//!   남는지 고정한다.
//! - 기존 구현 및 제약 조건: CI는 MSRV(1.88), 고정 툴체인(1.99.0), 비차단 최신 stable에서
//!   cargo test를 돌린다. rustc 진단 문구는 버전마다 달라 한 스냅샷이 세 툴체인을 모두 통과할
//!   수 없다.
//! - 검토한 주요 대안: (1) #[ignore] 후 --ignored로 실행, (2) 툴체인 버전 감지 후 건너뛰기,
//!   (3) 환경 변수 DISCORDRS_TRYBUILD=1일 때만 실행, (4) doctest의 compile_fail.
//! - 선택한 방식: (3).
//! - 다른 대안 대신 이 방식을 선택한 이유: (1)은 기존 라이브 DAVE 테스트처럼 --ignored를 쓰는
//!   다른 테스트까지 함께 돌린다. (2)는 버전 문자열 파싱이 필요하고 의도가 숨는다. (4)는 "어떤
//!   오류로" 실패하는지 확인하지 않아 오타로 실패해도 통과한다. 환경 변수는 CI가 고정
//!   툴체인 작업에서만 켜면 된다.
//! - 장점, 단점 및 영향: 기본 cargo test에서는 건너뛰므로 로컬에서 놓치기 쉽다. 그래서 CI
//!   rust-check 작업(고정 툴체인)의 "Compile-fail snapshots" 단계가 환경 변수를 켜서 항상
//!   돌린다. msrv·stable-canary는 파일만 컴파일하고 건너뛴다. 툴체인을 올릴 때 스냅샷도
//!   갱신한다(discordrsdocs/docs/guide/testing-and-coverage.md).
//! ```

#[test]
fn misuse_of_typed_ids_and_patches_does_not_compile() {
    if std::env::var_os("DISCORDRS_TRYBUILD").is_none_or(|value| value != "1") {
        eprintln!(
            "skipping compile-fail snapshots: set DISCORDRS_TRYBUILD=1 and use the pinned \
             CI toolchain (see tests/compile_fail.rs)"
        );
        return;
    }
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/ui/*.rs");
}
