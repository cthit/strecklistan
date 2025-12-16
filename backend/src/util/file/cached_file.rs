use rocket::Request;
use rocket::fs::NamedFile;
use rocket::http::{
    Header, Status,
    hyper::header::{CACHE_CONTROL, ETAG},
};
use rocket::response::{Responder, Response};
use std::io;
use std::path::Path;

#[derive(Debug)]
pub struct CachedFile {
    etag: String,
    cache_control: String,
    file: Option<NamedFile>,
}

impl CachedFile {
    pub async fn open<P: AsRef<Path>>(
        path: P,
        req_etag: Option<String>,
        cache_control: String,
    ) -> io::Result<CachedFile> {
        let path = path.as_ref();
        let etag = generate_etag(path);
        let file = match req_etag {
            Some(req_etag) if req_etag == etag => None,
            _ => Some(NamedFile::open(path).await?),
        };

        Ok(CachedFile {
            etag,
            cache_control,
            file,
        })
    }
}

impl<'r> Responder<'r, 'static> for CachedFile {
    fn respond_to(self, req: &'r Request) -> Result<Response<'static>, Status> {
        let mut response = match self.file {
            None => {
                let mut response = Response::new();
                response.set_status(Status::NotModified);
                response
            }
            Some(file) => file.respond_to(req)?,
        };

        response.adjoin_header(Header::new(ETAG.as_str(), self.etag));
        response.adjoin_header(Header::new(CACHE_CONTROL.as_str(), self.cache_control));

        Ok(response)
    }
}

fn generate_etag<P: AsRef<Path>>(path: P) -> String {
    use chrono::{DateTime, Utc};
    use lazy_static::lazy_static;
    use sha2::{Digest, Sha256};
    use std::os::unix::ffi::OsStrExt;

    lazy_static! {
        static ref START_TIME: DateTime<Utc> = Utc::now();
    }

    let mut hasher = Sha256::new();
    hasher.update(path.as_ref().as_os_str().as_bytes());
    hasher.update((*START_TIME).timestamp().to_be_bytes());

    let hash = hasher.finalize();
    hex::encode(hash)
}
