use crate::util::{ord::OrdL, StatusJson};
use log::error;
use rocket::http::{ContentType, MediaType, Status};
use rocket::outcome::Outcome;
use rocket::request::{self, FromRequest, Request};
use rocket::response::{self, Responder};
use serde::Serialize;
use std::cmp::Reverse;
use std::error::Error;
use strum::IntoEnumIterator;
use strum_macros::EnumIter;

/// Serialize the response as some supported format (specified by the client)
///
/// The client can specify the desired format using the `Accept`-header.
///
/// ## Supported formats
/// - JSON (default)
/// - RON
/// - MessagePack
///
/// ## Usage
/// ```
/// #[derive(Serialize)]
/// struct MyStruct {
///     hello: &'static str,
/// }
///
/// fn route(accept: SerAccept) -> Ser<MyStruct> {
///     accept.ser(MyStruct { hello: "there" })
/// }
/// ```
pub struct Ser<T> {
    encoding: Encoding,
    value: T,
}

/// Validate the `Accept`-header to decide whether
/// the client can accept the serialized response.
#[derive(Clone, Copy)]
pub struct SerAccept {
    encoding: Encoding,
}

#[derive(Clone, Copy, EnumIter)]
enum Encoding {
    // NOTE: the default is the first element
    Json,
    Ron,
    MsgPack,
}

/// https://developer.mozilla.org/en-US/docs/Glossary/Quality_values
const DEFAULT_QUALITY_WEIGHT: f32 = 1.0;

impl<'r, 'o, T> Responder<'r, 'o> for Ser<T>
where
    'o: 'r,
    T: Serialize,
{
    fn respond_to(self, request: &'r Request<'_>) -> response::Result<'o> {
        let bytes = err_500(self.encoding.serialize(&self.value))?;

        let content_type = ContentType(self.encoding.mime());

        (content_type, bytes).respond_to(request)
    }
}

fn err_500<T, E: std::fmt::Display>(result: Result<T, E>) -> Result<T, Status> {
    result.map_err(|e| {
        error!("error serializing response: {}", e);
        Status::InternalServerError
    })
}

impl SerAccept {
    pub fn ser<T>(self, value: T) -> Ser<T> {
        Ser {
            value,
            encoding: self.encoding,
        }
    }
}

#[rocket::async_trait]
impl<'a> FromRequest<'a> for SerAccept {
    type Error = StatusJson;

    async fn from_request(request: &'a Request<'_>) -> request::Outcome<Self, Self::Error> {
        // extract all accept types supported by the client
        let mut accepted: Vec<_> = request
            .accept()
            .iter()
            .flat_map(|accept| accept.iter())
            .collect();

        if accepted.is_empty() {
            let default = Encoding::iter().next().unwrap();
            Outcome::Success(SerAccept { encoding: default })
        } else {
            // sort by client preference
            accepted.sort_by_key(|media| OrdL(Reverse(media.weight_or(DEFAULT_QUALITY_WEIGHT))));

            // check if we can provide any of the provided content types
            accepted
                .iter()
                .flat_map(|accept| Encoding::iter().map(move |encoding| (accept, encoding)))
                .find(|(accept, encoding)| encoding.matches(accept.media_type()))
                .map(|(_, encoding)| Outcome::Success(SerAccept { encoding }))
                .unwrap_or_else(|| {
                    // if not, return error
                    let status = Status::NotAcceptable;
                    Outcome::Error((status, status.into()))
                })
        }
    }
}

impl Encoding {
    const fn mime(&self) -> MediaType {
        match self {
            Encoding::Json => MediaType::JSON,
            Encoding::Ron => MediaType::const_new("application", "ron", &[]),
            Encoding::MsgPack => MediaType::MsgPack,
        }
    }

    fn serialize<T: Serialize>(&self, value: &T) -> Result<Vec<u8>, Box<dyn Error>> {
        Ok(match self {
            Encoding::Json => serde_json::to_vec(value)?,
            Encoding::Ron => {
                use ron::ser::{to_string_pretty, PrettyConfig};
                to_string_pretty(value, PrettyConfig::default()).map(|s| s.into_bytes())?
            }
            Encoding::MsgPack => rmp_serde::to_vec(value)?,
        })
    }

    fn matches(&self, media: &MediaType) -> bool {
        if media.sub() == "*" {
            if media.top() == "*" {
                true // */* always matches
            } else {
                media.top() == self.mime().top()
            }
        } else {
            media == &self.mime()
        }
    }
}
