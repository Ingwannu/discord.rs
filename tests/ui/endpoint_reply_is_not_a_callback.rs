// `EndpointReply::Accepted` means "answer Discord's HTTP request with 202 and
// no body"; it is not an interaction callback. `EndpointReply` has no
// `Serialize`, and the REST callback takes an `InteractionResponse`, so the
// 202 marker cannot be encoded or posted as a callback body.
use discordrs::request::EndpointReply;
use discordrs::{InteractionId, RestClient};

fn main() {
    let _body = serde_json::to_vec(&EndpointReply::Accepted);

    let http = RestClient::new("token", None);
    let id: InteractionId = "1".parse().unwrap();
    let _sent = http.create_interaction_response(id, "token", &EndpointReply::Accepted);
}
