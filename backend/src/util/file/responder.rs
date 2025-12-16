use crate::util::CachedFile;
use either::Either;
use rocket::fairing::{Fairing, Info, Kind};
use rocket::fs::NamedFile;
use rocket::http::Status;
use rocket::http::hyper::header::IF_NONE_MATCH;
use rocket::response::Responder;
use rocket::{Request, Response};
use std::path::{Path, PathBuf};

/// A Fairing that intercepts 404-responses and tries to return a file at that path instead
pub struct FileResponder {
    pub folder: &'static str,

    /// Enable HTTP cache-control
    pub enable_cache: bool,

    /// Max age of a cached file before the client should re-fetch
    pub max_age: usize,
}

#[rocket::async_trait]
impl Fairing for FileResponder {
    fn info(&self) -> Info {
        Info {
            name: "File Responder",
            kind: Kind::Response,
        }
    }

    async fn on_response<'r>(&self, request: &'r Request<'_>, response: &mut Response<'r>) {
        // Don't change a successful user's response, ever.
        if response.status() != Status::NotFound {
            return;
        }

        let first_segment = request.uri().path().segments().next();

        // ignore requests to /api/...
        if first_segment == Some("api") {
            return;
        }

        let path: Option<PathBuf> = request
            .uri()
            .path()
            .segments()
            // make sure path is bullshit-free
            .map(|s| (!s.contains('/') && s != ".." || s != ".").then_some(s))
            .collect();
        let path = match path {
            Some(path) => path,
            None => return,
        };

        let root_path = Path::new(self.folder);
        let mut path = root_path.join(path);
        assert!(!path.has_root());

        async fn open_file(
            path: &Path,
            request: &Request<'_>,
            opt: &FileResponder,
        ) -> Option<Either<CachedFile, NamedFile>> {
            if !path.is_file() {
                return None;
            }

            Some(if opt.enable_cache {
                let req_etag = request
                    .headers()
                    .get_one(IF_NONE_MATCH.as_str())
                    .map(|etag| etag.to_string());
                let cache_control = format!("must-revalidate, max-age={}", opt.max_age);

                Either::Left(
                    CachedFile::open(path.to_owned(), req_etag, cache_control)
                        .await
                        .ok()?,
                )
            } else {
                Either::Right(NamedFile::open(path).await.ok()?)
            })
        }

        // try to serve a file at that path
        let file = match open_file(&path, request, self).await {
            Some(file) => file,
            None => {
                // if that failed, we check if we're only one path segment deep
                let num_segments = request.uri().path().segments().len();
                if num_segments > 1 {
                    return;
                }

                // and try to return index.html
                path = root_path.join("index.html");
                match open_file(&path, request, self).await {
                    Some(file) => file,
                    None => return,
                }
            }
        };

        match file.respond_to(request) {
            Err(_) => return,
            Ok(new_response) => {
                info!(
                    "FileResponder: intercepted 404, responding with file {:?}",
                    path
                );
                *response = new_response;
            }
        }
    }
}
