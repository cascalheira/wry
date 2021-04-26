// Copyright 2019-2021 Tauri Programme within The Commons Conservancy
// SPDX-License-Identifier: Apache-2.0
// SPDX-License-Identifier: MIT

use std::fmt;

const MIMETYPE_PLAIN: &str = "text/plain";

/// [Web Compatible MimeTypes](https://developer.mozilla.org/en-US/docs/Web/HTTP/Basics_of_HTTP/MIME_types#important_mime_types_for_web_developers)
pub(crate) enum MimeType {
  CSS,
  CSV,
  HTML,
  ICO,
  JS,
  JSON,
  JSONLD,
  OCTETSTREAM,
  RTF,
  SVG,
}

impl std::fmt::Display for MimeType {
  fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
    let mime = match self {
      MimeType::CSS => "text/css",
      MimeType::CSV => "text/csv",
      MimeType::HTML => "text/html",
      MimeType::ICO => "image/vnd.microsoft.icon",
      MimeType::JS => "text/javascript",
      MimeType::JSON => "application/json",
      MimeType::JSONLD => "application/ld+json",
      MimeType::OCTETSTREAM => "application/octet-stream",
      MimeType::RTF => "application/rtf",
      MimeType::SVG => "image/svg+xml",
    };
    write!(f, "{}", mime)
  }
}

impl MimeType {
  /// parse a URI suffix to convert text/plain mimeType to their actual web compatible mimeType.
  pub fn parse_from_uri(uri: &str) -> MimeType {
    let suffix = uri.split(".").last();
    match suffix {
      Some("bin") => Self::OCTETSTREAM,
      Some("css") => Self::CSS,
      Some("csv") => Self::CSV,
      Some("html") => Self::HTML,
      Some("ico") => Self::ICO,
      Some("js") => Self::JS,
      Some("json") => Self::JSON,
      Some("jsonld") => Self::JSONLD,
      Some("rtf") => Self::RTF,
      Some("svg") => Self::SVG,
      // Assume HTML when a TLD is found for eg. `wry:://tauri.studio` | `wry://hello.com`
      Some(_) => Self::HTML,
      // https://developer.mozilla.org/en-US/docs/Web/HTTP/Basics_of_HTTP/MIME_types/Common_types
      // using octet stream according to this:
      None => Self::OCTETSTREAM,
    }
  }

  /// infer mimetype from content (or) URI if needed.
  pub fn parse(content: &Vec<u8>, uri: &str) -> String {
    let mime = match infer::get(&content) {
      Some(info) => info.mime_type(),
      None => MIMETYPE_PLAIN,
    };

    if mime == MIMETYPE_PLAIN {
      return Self::parse_from_uri(uri).to_string();
    }

    mime.to_string()
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn should_parse_mimetype_from_uri() {
    let css = MimeType::parse_from_uri(
      "https://unpkg.com/browse/bootstrap@4.1.0/dist/css/bootstrap-grid.css",
    )
    .to_string();
    assert_eq!(css, "text/css".to_string());

    let csv: String = MimeType::parse_from_uri("https://example.com/random.csv").to_string();
    assert_eq!(csv, "text/csv".to_string());

    let ico: String =
      MimeType::parse_from_uri("https://icons.duckduckgo.com/ip3/microsoft.com.ico").to_string();
    assert_eq!(ico, String::from("image/vnd.microsoft.icon"));

    let html: String = MimeType::parse_from_uri("https://tauri.studio/index.html").to_string();
    assert_eq!(html, String::from("text/html"));

    let js: String =
      MimeType::parse_from_uri("https://unpkg.com/react@17.0.1/umd/react.production.min.js")
        .to_string();
    assert_eq!(js, "text/javascript".to_string());

    let json: String =
      MimeType::parse_from_uri("https://unpkg.com/browse/react@17.0.1/build-info.json").to_string();
    assert_eq!(json, String::from("application/json"));

    let jsonld: String = MimeType::parse_from_uri("https:/example.com/hello.jsonld").to_string();
    assert_eq!(jsonld, String::from("application/ld+json"));

    let rtf: String = MimeType::parse_from_uri("https://example.com/document.rtf").to_string();
    assert_eq!(rtf, String::from("application/rtf"));

    let svg: String = MimeType::parse_from_uri("https://example.com/picture.svg").to_string();
    assert_eq!(svg, String::from("image/svg"));

    let custom_scheme = MimeType::parse_from_uri("wry://tauri.studio").to_string();
    assert_eq!(custom_scheme, String::from("text/html"));
  }
}
