use diesel::result::Error as DieselError;
use duplicate::duplicate_item;
use log::{info, warn};
use rocket::Request;
use rocket::http::Status;
use rocket::response::{Responder, Response};
use rocket::serde::json::{Json, json}; // macro

/// An error message which can be serialized as JSON.
///
/// #### Example JSON
/// ```json
/// {
///   "status": 404,
///   "description": "Not Found"
/// }
/// ```
#[derive(Debug, Clone)]
pub struct StatusJson {
    pub status: Status,
    pub description: String,
}

impl StatusJson {
    pub fn new<S: ToString>(status: Status, description: S) -> Self {
        StatusJson {
            status,
            description: description.to_string(),
        }
    }

    pub fn describe<S: ToString>(mut self, description: S) -> Self {
        self.description = description.to_string();
        self
    }
}

impl<'r> Responder<'r, 'static> for StatusJson {
    fn respond_to(self, req: &'r Request) -> Result<Response<'static>, Status> {
        if self.status.code >= 400 {
            warn!(
                "Responding with status {}.\n\
                 Description: {}",
                self.status, self.description,
            );
        } else {
            info!("Responding with status {}", self.status);
        }

        let mut response = Json(json!({
            "status": self.status.code,
            "description": self.description,
        }))
        .respond_to(req)?;

        response.set_status(self.status);

        Ok(response)
    }
}

#[duplicate_item(
  status_code                     T;
  [ Status::BadRequest ]          [ r2d2::Error ];
  [ Status::InternalServerError ] [ diesel::ConnectionError ];
)]
impl From<T> for StatusJson {
    fn from(e: T) -> StatusJson {
        StatusJson {
            status: status_code,
            description: e.to_string(),
        }
    }
}

impl From<Status> for StatusJson {
    fn from(status: Status) -> StatusJson {
        StatusJson {
            description: status.reason().unwrap_or("").to_string(),
            status,
        }
    }
}

impl From<DieselError> for StatusJson {
    fn from(e: DieselError) -> StatusJson {
        match e {
            DieselError::NotFound => StatusJson {
                status: Status::NotFound,
                description: "Not Found in Database".into(),
            },
            err => StatusJson {
                status: Status::InternalServerError,
                description: err.to_string(),
            },
        }
    }
}
