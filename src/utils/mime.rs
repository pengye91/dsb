// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! MIME type detection utilities
//!
//! This module provides utilities for detecting MIME types based on file extensions.

use std::path::Path;

/// Detect MIME type based on file extension
///
/// # Arguments
///
/// * `file_path` - Path to the file (can be relative or absolute)
///
/// # Returns
///
/// MIME type string (e.g., "text/html", "image/png")
///
/// # Examples
///
/// ```
/// use dsb::utils::mime::detect_mime_type;
///
/// assert_eq!(detect_mime_type("index.html"), "text/html");
/// assert_eq!(detect_mime_type("style.css"), "text/css");
/// assert_eq!(detect_mime_type("app.js"), "application/javascript");
/// assert_eq!(detect_mime_type("data.json"), "application/json");
/// assert_eq!(detect_mime_type("image.png"), "image/png");
/// assert_eq!(detect_mime_type("unknown.xyz"), "application/octet-stream");
/// ```
pub fn detect_mime_type(file_path: &str) -> &'static str {
    let path = Path::new(file_path);
    let ext = path.extension().and_then(|e| e.to_str());

    match ext {
        // HTML
        Some("html") | Some("htm") => "text/html",

        // CSS
        Some("css") => "text/css",

        // JavaScript
        Some("js") | Some("mjs") | Some("cjs") => "application/javascript",

        // JSON
        Some("json") => "application/json",

        // XML
        Some("xml") | Some("xsl") | Some("xsd") => "application/xml",

        // Images
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") | Some("jpe") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("svg") | Some("svgz") => "image/svg+xml",
        Some("ico") => "image/x-icon",
        Some("webp") => "image/webp",
        Some("bmp") => "image/bmp",
        Some("tiff") | Some("tif") => "image/tiff",

        // Fonts
        Some("woff") => "font/woff",
        Some("woff2") => "font/woff2",
        Some("ttf") => "font/ttf",
        Some("otf") => "font/otf",
        Some("eot") => "application/vnd.ms-fontobject",

        // Text
        Some("txt") => "text/plain",
        Some("md") | Some("markdown") => "text/markdown",
        Some("csv") => "text/csv",

        // PDF
        Some("pdf") => "application/pdf",

        // Archives
        Some("zip") => "application/zip",
        Some("tar") => "application/x-tar",
        Some("gz") | Some("gzip") => "application/gzip",
        Some("rar") => "application/vnd.rar",
        Some("7z") => "application/x-7z-compressed",

        // Web
        Some("wasm") => "application/wasm",
        Some("webmanifest") => "application/manifest+json",

        // Media
        Some("mp3") => "audio/mpeg",
        Some("wav") => "audio/wav",
        Some("ogg") => "audio/ogg",
        Some("mp4") => "video/mp4",
        Some("webm") => "video/webm",
        Some("avi") => "video/x-msvideo",

        // Default binary
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_html_mime_types() {
        assert_eq!(detect_mime_type("index.html"), "text/html");
        assert_eq!(detect_mime_type("page.htm"), "text/html");
    }

    #[test]
    fn test_css_mime_type() {
        assert_eq!(detect_mime_type("style.css"), "text/css");
    }

    #[test]
    fn test_javascript_mime_types() {
        assert_eq!(detect_mime_type("app.js"), "application/javascript");
        assert_eq!(detect_mime_type("module.mjs"), "application/javascript");
        assert_eq!(detect_mime_type("common.cjs"), "application/javascript");
    }

    #[test]
    fn test_json_mime_type() {
        assert_eq!(detect_mime_type("data.json"), "application/json");
    }

    #[test]
    fn test_image_mime_types() {
        assert_eq!(detect_mime_type("image.png"), "image/png");
        assert_eq!(detect_mime_type("photo.jpg"), "image/jpeg");
        assert_eq!(detect_mime_type("photo.jpeg"), "image/jpeg");
        assert_eq!(detect_mime_type("animation.gif"), "image/gif");
        assert_eq!(detect_mime_type("icon.svg"), "image/svg+xml");
        assert_eq!(detect_mime_type("favicon.ico"), "image/x-icon");
    }

    #[test]
    fn test_font_mime_types() {
        assert_eq!(detect_mime_type("font.woff"), "font/woff");
        assert_eq!(detect_mime_type("font.woff2"), "font/woff2");
        assert_eq!(detect_mime_type("font.ttf"), "font/ttf");
    }

    #[test]
    fn test_text_mime_types() {
        assert_eq!(detect_mime_type("readme.txt"), "text/plain");
        assert_eq!(detect_mime_type("doc.md"), "text/markdown");
    }

    #[test]
    fn test_pdf_mime_type() {
        assert_eq!(detect_mime_type("doc.pdf"), "application/pdf");
    }

    #[test]
    fn test_archive_mime_types() {
        assert_eq!(detect_mime_type("files.zip"), "application/zip");
        assert_eq!(detect_mime_type("archive.tar"), "application/x-tar");
        assert_eq!(detect_mime_type("data.gz"), "application/gzip");
    }

    #[test]
    fn test_unknown_extension() {
        assert_eq!(detect_mime_type("file.xyz"), "application/octet-stream");
    }

    #[test]
    fn test_no_extension() {
        assert_eq!(detect_mime_type("Makefile"), "application/octet-stream");
    }

    #[test]
    fn test_path_with_directories() {
        assert_eq!(detect_mime_type("/path/to/index.html"), "text/html");
        assert_eq!(detect_mime_type("css/styles/main.css"), "text/css");
    }

    #[test]
    fn test_case_sensitive_extensions() {
        // Extensions are case-sensitive on Unix, case-insensitive on Windows
        // This implementation treats them as case-sensitive (Unix behavior)
        assert_eq!(detect_mime_type("file.HTML"), "application/octet-stream");
        assert_eq!(detect_mime_type("file.Html"), "application/octet-stream");
    }
}
