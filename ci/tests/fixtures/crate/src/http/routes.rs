// Fixture route manifest: the rows ci/api_drift.py reads instead of scanning
// call sites. The macro definition below must not be mistaken for the table.
macro_rules! route_manifest {
    ($($row:tt)*) => {};
}

route_manifest! {
    // Comment lines between rows are allowed.
    GetGuildWidget: GET "/guilds/{guild.id}/widget" bot, query [], body () => Value, api [get_widget];
    ModifyGuildWidget: PATCH "/guilds/{guild.id}/widget" bot, query [reason], body Value => Value, api [modify_widget];
    GetChannelMessage: GET "/channels/{channel.id}/messages/{message.id}" bot, query [], body () => Value, api [get_message];
    GetPinnedMessages: GET "/channels/{channel.id}/pins" bot, query [limit], body () => Value, api [get_pins, get_pins_limited], was [pins => get_pins];
    GetSparkle: GET "/sparkles/{sparkle.id}" bot, query [], body () => Value, api [get_sparkle];
    ExecuteWebhook: POST "/webhooks/{webhook.id}/{webhook.token}" token, query [wait], body Value => Value, api [execute_widget_webhook];
    LegacyThing: POST "/legacy/thing" bot, query [], body () => Value, api [removed_upstream];
    OAuth2Token: POST "/oauth2/token" client, query [], body () => Token, oauth2 [exchange_code];
}
