//! Interaction callbacks and modals on the wire (`discordrs::request`),
//! against discord-api-docs `3714862` (`interactions/receiving-and-responding.mdx`
//! "Interaction Response Object", `components/reference.mdx` Label, Text
//! Input, selects, File Upload, Radio Group, Checkbox Group, Checkbox, Text
//! Display): a byte golden for every callback variant and every modal
//! child (numeric `type` first, no `Value` on the way), and the modal
//! validation rules. The compile-time half (a modal child cannot go into a
//! message row, `EndpointReply` is not a callback body, the legacy action
//! row only through its deprecated constructor) is in `tests/ui/`.

#![allow(
    clippy::expect_used,
    clippy::panic,
    reason = "a failed expectation fails the test"
)]

use discordrs::request::{
    ChannelSelect, Checkbox, CheckboxGroup, EndpointReply, FileUpload, InteractionMessage,
    InteractionResponse, Label, LabelComponent, LegacyTextInputRow, MentionableSelect, Modal,
    ModalComponent, RadioGroup, RawComponent, RawInteractionResponse, RoleSelect, StringSelect,
    TextDisplay, TextInput, UpdateMessage, UserSelect,
};
use discordrs::{
    ApplicationCommandOptionChoice, ChannelType, ComponentId, DiscordError,
    InteractionResponseType, SelectOption,
};

fn wire(value: &impl serde::Serialize) -> String {
    serde_json::to_string(value).expect("encodes")
}

fn modal_error(modal: &Modal) -> String {
    match modal.validate() {
        Err(DiscordError::Model { message }) => message,
        other => panic!("expected a model error, got {other:?}"),
    }
}

fn text_label() -> Label {
    Label::new("Name", TextInput::short("name"))
}

// --- callbacks ---------------------------------------------------------------

#[test]
fn every_callback_variant_has_a_byte_golden() {
    let cases: Vec<(InteractionResponse, InteractionResponseType, &str)> = vec![
        (
            InteractionResponse::Pong,
            InteractionResponseType::PONG,
            r#"{"type":1}"#,
        ),
        (
            InteractionResponse::reply("hi"),
            InteractionResponseType::CHANNEL_MESSAGE_WITH_SOURCE,
            r#"{"type":4,"data":{"content":"hi"}}"#,
        ),
        (
            InteractionResponse::reply(InteractionMessage::from("psst").ephemeral(true)),
            InteractionResponseType::CHANNEL_MESSAGE_WITH_SOURCE,
            r#"{"type":4,"data":{"content":"psst","flags":64}}"#,
        ),
        (
            InteractionResponse::defer(false),
            InteractionResponseType::DEFERRED_CHANNEL_MESSAGE_WITH_SOURCE,
            r#"{"type":5,"data":{"flags":0}}"#,
        ),
        (
            InteractionResponse::DeferredChannelMessage { ephemeral: true },
            InteractionResponseType::DEFERRED_CHANNEL_MESSAGE_WITH_SOURCE,
            r#"{"type":5,"data":{"flags":64}}"#,
        ),
        (
            InteractionResponse::DeferredUpdateMessage,
            InteractionResponseType::DEFERRED_UPDATE_MESSAGE,
            r#"{"type":6}"#,
        ),
        (
            InteractionResponse::update(UpdateMessage::new().components(Vec::new())),
            InteractionResponseType::UPDATE_MESSAGE,
            r#"{"type":7,"data":{"components":[]}}"#,
        ),
        (
            // A text message becoming Components V2: `content: null` clears
            // the text, and the patch writes nothing else.
            InteractionResponse::update(UpdateMessage::new().clear_content()),
            InteractionResponseType::UPDATE_MESSAGE,
            r#"{"type":7,"data":{"content":null}}"#,
        ),
        (
            InteractionResponse::update("edited"),
            InteractionResponseType::UPDATE_MESSAGE,
            r#"{"type":7,"data":{"content":"edited"}}"#,
        ),
        (
            InteractionResponse::autocomplete([
                ApplicationCommandOptionChoice::new("Red", "red"),
                ApplicationCommandOptionChoice::new("Two", 2),
            ]),
            InteractionResponseType::APPLICATION_COMMAND_AUTOCOMPLETE_RESULT,
            r#"{"type":8,"data":{"choices":[{"name":"Red","value":"red"},{"name":"Two","value":2}]}}"#,
        ),
        (
            InteractionResponse::AutocompleteResult(Vec::new()),
            InteractionResponseType::APPLICATION_COMMAND_AUTOCOMPLETE_RESULT,
            r#"{"type":8,"data":{"choices":[]}}"#,
        ),
        (
            InteractionResponse::modal(Modal::new("m", "Title").component(text_label())),
            InteractionResponseType::MODAL,
            r#"{"type":9,"data":{"custom_id":"m","title":"Title","components":[{"type":18,"label":"Name","component":{"type":4,"custom_id":"name","style":1}}]}}"#,
        ),
        (
            InteractionResponse::LaunchActivity,
            InteractionResponseType::LAUNCH_ACTIVITY,
            r#"{"type":12}"#,
        ),
        (
            InteractionResponse::Raw(RawInteractionResponse::new(InteractionResponseType::new(
                13,
            ))),
            InteractionResponseType::new(13),
            r#"{"type":13}"#,
        ),
        (
            InteractionResponse::Raw(
                RawInteractionResponse::new(InteractionResponseType::CHANNEL_MESSAGE_WITH_SOURCE)
                    .data_json(r#"{"content":"raw","future_field":[1, 2]}"#)
                    .expect("valid JSON"),
            ),
            InteractionResponseType::CHANNEL_MESSAGE_WITH_SOURCE,
            // Raw data is sent exactly as written, spacing included.
            r#"{"type":4,"data":{"content":"raw","future_field":[1, 2]}}"#,
        ),
    ];
    for (response, kind, golden) in cases {
        assert_eq!(response.kind(), kind, "{golden}");
        assert_eq!(wire(&response), golden);
    }
}

#[test]
#[expect(deprecated, reason = "the deprecated callback type must stay sendable")]
fn premium_required_is_deprecated_but_still_sendable() {
    let response = InteractionResponse::PremiumRequired;
    assert_eq!(response.kind(), InteractionResponseType::PREMIUM_REQUIRED);
    assert_eq!(wire(&response), r#"{"type":10}"#);
}

#[test]
fn raw_data_must_be_json_and_can_come_from_any_serializable_value() {
    let kind = InteractionResponseType::CHANNEL_MESSAGE_WITH_SOURCE;
    assert!(RawInteractionResponse::new(kind)
        .data_json("{not json")
        .is_err());
    let raw = RawInteractionResponse::new(kind)
        .data(&serde_json::json!({ "content": "from a value" }))
        .expect("serializes");
    assert_eq!(
        wire(&InteractionResponse::Raw(raw)),
        r#"{"type":4,"data":{"content":"from a value"}}"#
    );
}

#[test]
fn endpoint_reply_boxes_the_inline_callback() {
    let reply: EndpointReply = InteractionResponse::defer(true).into();
    match reply {
        EndpointReply::Inline(response) => {
            assert_eq!(wire(&*response), r#"{"type":5,"data":{"flags":64}}"#);
        }
        EndpointReply::Accepted => panic!("expected an inline reply"),
    }
    let reply: EndpointReply = Modal::new("m", "M").into();
    assert!(matches!(
        reply,
        EndpointReply::Inline(response) if response.kind() == InteractionResponseType::MODAL
    ));
}

#[test]
fn autocomplete_validation_follows_the_choice_limits() {
    let choice = |name: &str| ApplicationCommandOptionChoice::new(name, "v");
    InteractionResponse::autocomplete((0..25).map(|index| choice(&index.to_string())))
        .validate()
        .expect("25 choices");
    let error = InteractionResponse::autocomplete((0..26).map(|index| choice(&index.to_string())))
        .validate()
        .expect_err("26 choices");
    assert!(error.to_string().contains("at most 25"), "{error}");
    assert!(InteractionResponse::autocomplete([choice("")])
        .validate()
        .is_err());
    assert!(
        InteractionResponse::autocomplete([ApplicationCommandOptionChoice::new(
            "long",
            "v".repeat(101)
        )])
        .validate()
        .is_err()
    );
    assert!(
        InteractionResponse::autocomplete([ApplicationCommandOptionChoice::new("nan", f64::NAN)])
            .validate()
            .is_err()
    );
}

// --- modal children -----------------------------------------------------------

#[test]
fn every_label_child_has_a_byte_golden() {
    let cases: Vec<(LabelComponent, &str)> = vec![
        (
            TextInput::paragraph("bio")
                .id(ComponentId::new(2))
                .min_length(1)
                .max_length(400)
                .required(false)
                .value("seed")
                .placeholder("About you")
                .into(),
            r#"{"type":4,"id":2,"custom_id":"bio","style":2,"min_length":1,"max_length":400,"required":false,"value":"seed","placeholder":"About you"}"#,
        ),
        (
            {
                let mut select = StringSelect::new("color");
                select.options = vec![SelectOption::new("Red", "red")];
                select.required = Some(true);
                select.into()
            },
            r#"{"type":3,"custom_id":"color","required":true,"options":[{"label":"Red","value":"red"}]}"#,
        ),
        (
            {
                let mut select = UserSelect::new("who");
                select.default_values = vec!["80351110224678912".parse().expect("a valid ID")];
                select.into()
            },
            r#"{"type":5,"custom_id":"who","default_values":[{"id":"80351110224678912","type":"user"}]}"#,
        ),
        (
            RoleSelect::new("role").into(),
            r#"{"type":6,"custom_id":"role"}"#,
        ),
        (
            MentionableSelect::new("ping").into(),
            r#"{"type":7,"custom_id":"ping"}"#,
        ),
        (
            {
                let mut select = ChannelSelect::new("where");
                select.channel_types = Some(vec![ChannelType::GUILD_TEXT]);
                select.into()
            },
            r#"{"type":8,"custom_id":"where","channel_types":[0]}"#,
        ),
        (
            FileUpload::new("files")
                .min_values(1)
                .max_values(3)
                .required(true)
                .file_types(["image", ".pdf"])
                .into(),
            r#"{"type":19,"custom_id":"files","min_values":1,"max_values":3,"required":true,"file_types":["image",".pdf"]}"#,
        ),
        (
            RadioGroup::new("size")
                .option(SelectOption::new("Small", "s"))
                .option(SelectOption::new("Large", "l").default_selected(true))
                .required(true)
                .into(),
            r#"{"type":21,"custom_id":"size","options":[{"label":"Small","value":"s"},{"label":"Large","value":"l","default":true}],"required":true}"#,
        ),
        (
            CheckboxGroup::new("days")
                .option(SelectOption::new("Mon", "mon"))
                .min_values(0)
                .max_values(1)
                .required(false)
                .into(),
            r#"{"type":22,"custom_id":"days","options":[{"label":"Mon","value":"mon"}],"min_values":0,"max_values":1,"required":false}"#,
        ),
        (
            Checkbox::new("tos").default_checked(false).into(),
            r#"{"type":23,"custom_id":"tos","default":false}"#,
        ),
        (
            RawComponent::from_json(r#"{"type":99,"custom_id":"future"}"#)
                .expect("valid JSON")
                .into(),
            r#"{"type":99,"custom_id":"future"}"#,
        ),
    ];
    for (component, golden) in cases {
        let label = Label::new("L", component)
            .description("D")
            .id(ComponentId::new(1));
        assert_eq!(
            wire(&label),
            format!(r#"{{"type":18,"id":1,"label":"L","description":"D","component":{golden}}}"#)
        );
    }
}

#[test]
fn top_level_modal_components_have_byte_goldens() {
    #[expect(deprecated, reason = "the legacy layout must stay sendable on purpose")]
    let legacy = LegacyTextInputRow::new("Old label", TextInput::short("old"));
    let modal = Modal::new("form", "Form")
        .component(text_label())
        .component(TextDisplay::new("**Read me**"))
        .component(legacy.id(ComponentId::new(7)))
        .component(RawComponent::from_json(r#"{"type":98}"#).expect("valid JSON"));
    assert_eq!(
        wire(&modal),
        concat!(
            r#"{"custom_id":"form","title":"Form","components":["#,
            r#"{"type":18,"label":"Name","component":{"type":4,"custom_id":"name","style":1}},"#,
            r#"{"type":10,"content":"**Read me**"},"#,
            // The deprecated layout is the only place a text input carries
            // its own `label`.
            r#"{"type":1,"id":7,"components":[{"type":4,"custom_id":"old","style":1,"label":"Old label"}]},"#,
            r#"{"type":98}]}"#
        )
    );
    modal.validate().expect("valid modal");
    assert!(matches!(
        modal.components.get(2),
        Some(ModalComponent::LegacyTextInputRow(_))
    ));
}

// --- modal validation --------------------------------------------------------

#[test]
fn modal_validation_follows_the_documented_limits() {
    let valid = || Modal::new("m", "T").component(text_label());
    valid().validate().expect("valid");

    assert!(modal_error(&Modal::new("m", "T")).contains("modal components must contain 1-5"));
    let mut six = Modal::new("m", "T");
    for index in 0..6 {
        six = six.component(Label::new("L", TextInput::short(format!("t{index}"))));
    }
    assert!(modal_error(&six).contains("got 6"));
    assert!(
        modal_error(&Modal::new("m", "t".repeat(46)).component(text_label()))
            .contains("modal title must be 1-45")
    );
    assert!(
        modal_error(&Modal::new("i".repeat(101), "T").component(text_label()))
            .contains("modal custom_id must be 1-100")
    );

    // Label text 1-45, description at most 100.
    let label = |label: Label| Modal::new("m", "T").component(label);
    label(Label::new("l".repeat(45), TextInput::short("a")).description("d".repeat(100)))
        .validate()
        .expect("limits are inclusive");
    let error = modal_error(&label(Label::new("l".repeat(46), TextInput::short("a"))));
    assert!(
        error.starts_with("modal components[0] (label): label text must be 1-45"),
        "{error}"
    );
    assert!(modal_error(&label(
        Label::new("L", TextInput::short("a")).description("d".repeat(101))
    ))
    .contains("label description must be at most 100"));

    // Text display content 1-4000.
    assert!(Modal::new("m", "T")
        .component(TextDisplay::new(""))
        .validate()
        .is_err());
}

#[test]
fn modal_selects_reject_disabled_and_a_required_zero_minimum() {
    let with_select =
        |select: StringSelect| Modal::new("m", "T").component(Label::new("S", select));
    let base = || {
        let mut select = StringSelect::new("s");
        select.options = vec![SelectOption::new("A", "a")];
        select
    };
    with_select(base()).validate().expect("plain select");

    // "Using `disabled` in a modal will result in an error."
    for disabled in [true, false] {
        let mut select = base();
        select.disabled = Some(disabled);
        assert!(modal_error(&with_select(select)).contains("disabled"));
    }

    // "`min_values` must be either omitted or at least `1` if `required` is
    // omitted or `true`."
    let mut select = base();
    select.min_values = Some(0);
    assert!(modal_error(&with_select(select.clone())).contains("min_values must be at least 1"));
    select.required = Some(false);
    with_select(select)
        .validate()
        .expect("optional select may allow 0");

    // The select's own limits still apply.
    let mut select = base();
    select.options.clear();
    assert!(modal_error(&with_select(select)).contains("options"));

    // Auto-populated selects get the same modal rules.
    let mut users = UserSelect::new("u");
    users.disabled = Some(true);
    assert!(
        modal_error(&Modal::new("m", "T").component(Label::new("U", users))).contains("disabled")
    );
}

#[test]
fn modal_rejects_repeated_custom_ids_and_component_ids() {
    let error = modal_error(
        &Modal::new("m", "T")
            .component(Label::new("A", TextInput::short("same")))
            .component(Label::new("B", Checkbox::new("same"))),
    );
    assert!(
        error.starts_with("modal components[1] (label): custom_id \"same\" is used more than once"),
        "{error}"
    );

    let error = modal_error(
        &Modal::new("m", "T")
            .component(Label::new("A", TextInput::short("a")).id(ComponentId::new(3)))
            .component({
                let mut text = TextDisplay::new("x");
                text.id = Some(ComponentId::new(3));
                text
            }),
    );
    assert!(
        error.contains("component id 3 is used more than once"),
        "{error}"
    );

    // `id: 0` means "let Discord assign one" and may repeat.
    Modal::new("m", "T")
        .component(Label::new("A", TextInput::short("a")).id(ComponentId::new(0)))
        .component(Label::new("B", TextInput::short("b")).id(ComponentId::new(0)))
        .validate()
        .expect("zero ids repeat");
}

#[test]
fn modal_inputs_check_their_documented_limits() {
    let one =
        |component: LabelComponent| Modal::new("m", "T").component(Label::new("L", component));

    // File upload: values 0-10, 0 only when not required, at most 10 types.
    one(FileUpload::new("f")
        .min_values(0)
        .required(false)
        .max_values(10)
        .into())
    .validate()
    .expect("0..=10");
    assert!(modal_error(&one(FileUpload::new("f").min_values(0).into()))
        .contains("file upload min_values must be at least 1"));
    assert!(
        modal_error(&one(FileUpload::new("f").max_values(11).into()))
            .contains("file upload max_values must be 1-10")
    );
    assert!(modal_error(&one(FileUpload::new("f")
        .file_types((0..11).map(|index| format!(".e{index}")))
        .into()))
    .contains("file_types"));

    // Radio group: 2-10 options, no emoji on an option.
    let two = || {
        RadioGroup::new("r")
            .option(SelectOption::new("A", "a"))
            .option(SelectOption::new("B", "b"))
    };
    one(two().into()).validate().expect("two options");
    assert!(modal_error(&one(RadioGroup::new("r")
        .option(SelectOption::new("A", "a"))
        .into()))
    .contains("radio group options must contain 2-10"));
    assert!(modal_error(&one(two()
        .option(SelectOption::new("C", "c").emoji("🙂"))
        .into()))
    .contains("emoji"));

    // Checkbox group: min_values above max_values.
    assert!(modal_error(&one(CheckboxGroup::new("c")
        .option(SelectOption::new("A", "a"))
        .option(SelectOption::new("B", "b"))
        .min_values(2)
        .max_values(1)
        .into()))
    .contains("must not exceed max_values"));

    // Text input lengths.
    assert!(
        modal_error(&one(TextInput::short("t").max_length(0).into()))
            .contains("text input max_length must be 1-4000")
    );
    assert!(modal_error(&one(TextInput::short("t")
        .min_length(9)
        .max_length(3)
        .into()))
    .contains("must not exceed max_length"));

    // Checkbox custom_id.
    assert!(modal_error(&one(Checkbox::new("").into())).contains("checkbox custom_id"));

    // Raw components are not inspected.
    one(RawComponent::from_json(r#"{"type":99}"#)
        .expect("valid JSON")
        .into())
    .validate()
    .expect("raw passes");
}

#[test]
fn legacy_action_row_checks_its_label() {
    #[expect(deprecated, reason = "covers the deprecated layout")]
    let row = LegacyTextInputRow::new("", TextInput::short("old"));
    assert!(modal_error(&Modal::new("m", "T").component(row)).contains("text input label"));
}

#[test]
fn response_conversions_and_validation_dispatch() {
    let response: InteractionResponse = Modal::new("m", "T").component(text_label()).into();
    assert_eq!(response.kind(), InteractionResponseType::MODAL);
    response.validate().expect("valid modal");
    // A modal response is checked through `Modal::validate`.
    assert!(InteractionResponse::from(Modal::new("m", "T"))
        .validate()
        .is_err());
    // Messages are checked by their own type; other callbacks have nothing
    // to check.
    InteractionResponse::reply("x".repeat(5000))
        .validate()
        .expect("not checked here");
    InteractionResponse::DeferredUpdateMessage
        .validate()
        .expect("nothing to check");

    let raw = RawInteractionResponse::new(InteractionResponseType::LAUNCH_ACTIVITY)
        .data_json(r#"{"a":1}"#)
        .expect("valid JSON");
    let debug = format!("{raw:?}");
    assert!(debug.contains("RawInteractionResponse"), "{debug}");
    assert!(debug.contains(r#"{\"a\":1}"#), "{debug}");
    let response: InteractionResponse = raw.into();
    assert_eq!(wire(&response), r#"{"type":12,"data":{"a":1}}"#);
}

#[test]
fn every_select_kind_gets_the_modal_rules() {
    let modal =
        |component: LabelComponent| Modal::new("m", "T").component(Label::new("L", component));
    let mut role = RoleSelect::new("role");
    let mut mentionable = MentionableSelect::new("ping");
    let mut channel = ChannelSelect::new("where");
    for select in [
        LabelComponent::from(role.clone()),
        mentionable.clone().into(),
        channel.clone().into(),
    ] {
        modal(select).validate().expect("valid select");
    }
    role.disabled = Some(false);
    mentionable.disabled = Some(true);
    channel.min_values = Some(0);
    assert!(modal(role.into()).validate().is_err());
    assert!(modal(mentionable.into()).validate().is_err());
    let error = modal(channel.into())
        .validate()
        .expect_err("required zero minimum");
    assert!(
        error.to_string().contains("min_values must be at least 1"),
        "{error}"
    );

    // A message row's select moves into a label unchanged.
    let mut string = StringSelect::new("s");
    string.options = vec![SelectOption::new("A", "a")];
    for (select, golden_type) in [
        (discordrs::request::SelectMenu::from(string), 3),
        (UserSelect::new("u").into(), 5),
        (RoleSelect::new("r").into(), 6),
        (MentionableSelect::new("m").into(), 7),
        (ChannelSelect::new("c").into(), 8),
    ] {
        let label = Label::new("L", LabelComponent::from(select));
        let json: serde_json::Value = serde_json::from_str(&wire(&label)).expect("JSON");
        assert_eq!(json["component"]["type"], golden_type);
    }
}

#[test]
fn errors_name_the_failing_component_kind_and_labels_validate_alone() {
    let error = Label::new("", TextInput::short("a"))
        .validate()
        .expect_err("empty label text");
    assert!(
        error.to_string().contains("label text must be 1-45"),
        "{error}"
    );
    Label::new("L", Checkbox::new("c"))
        .validate()
        .expect("valid label");

    assert!(
        modal_error(&Modal::new("m", "T").component(TextDisplay::new("")))
            .starts_with("modal components[0] (text display): ")
    );
    #[expect(deprecated, reason = "covers the deprecated layout")]
    let row = LegacyTextInputRow::new("L", TextInput::short("x").max_length(0));
    assert!(modal_error(&Modal::new("m", "T").component(row))
        .starts_with("modal components[0] (action row): text input max_length"));
}

#[test]
fn modal_inputs_carry_their_component_id() {
    let id = ComponentId::new(9);
    let cases: Vec<(LabelComponent, &str)> = vec![
        (
            FileUpload::new("f").id(id).into(),
            r#"{"type":19,"id":9,"custom_id":"f"}"#,
        ),
        (
            RadioGroup::new("r").id(id).into(),
            r#"{"type":21,"id":9,"custom_id":"r","options":[]}"#,
        ),
        (
            CheckboxGroup::new("g").id(id).into(),
            r#"{"type":22,"id":9,"custom_id":"g","options":[]}"#,
        ),
        (
            Checkbox::new("c").id(id).into(),
            r#"{"type":23,"id":9,"custom_id":"c"}"#,
        ),
    ];
    for (component, golden) in cases {
        assert_eq!(wire(&component), golden);
    }
}

/// Regression (astra review): `min_values` was compared with `max_values`
/// only when both were set, so `min_values(2)` alone on a file upload (whose
/// `max_values` defaults to 1) passed. An omitted side now takes Discord's
/// default.
#[test]
fn omitted_max_values_take_discords_default() {
    let one =
        |component: LabelComponent| Modal::new("m", "T").component(Label::new("L", component));

    // File upload: `max_values` defaults to 1.
    let error = modal_error(&one(FileUpload::new("f").min_values(2).into()));
    assert!(
        error.contains("file upload min_values (2) must not exceed max_values, which is 1 when omitted (Discord's default)"),
        "{error}"
    );
    one(FileUpload::new("f").min_values(1).into())
        .validate()
        .expect("min 1 with the default max 1");
    one(FileUpload::new("f").min_values(2).max_values(2).into())
        .validate()
        .expect("explicit max");
    // `min_values` defaults to 1, which any valid `max_values` allows.
    one(FileUpload::new("f").max_values(1).into())
        .validate()
        .expect("min omitted");

    // Checkbox group: `max_values` defaults to the number of options.
    let three = || {
        CheckboxGroup::new("c")
            .option(SelectOption::new("A", "a"))
            .option(SelectOption::new("B", "b"))
            .option(SelectOption::new("C", "c"))
    };
    one(three().min_values(3).into())
        .validate()
        .expect("min equal to the number of options");
    let error = modal_error(&one(CheckboxGroup::new("c")
        .option(SelectOption::new("A", "a"))
        .option(SelectOption::new("B", "b"))
        .min_values(3)
        .into()));
    assert!(
        error.contains("checkbox group min_values (3) must not exceed max_values, which is 2 when omitted (the number of options)"),
        "{error}"
    );
    // An explicit max is still not compared with the number of options.
    one(three().max_values(10).into())
        .validate()
        .expect("explicit max above the option count");
}
