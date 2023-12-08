use anyhow::Context;
use regex::Regex;
use zip::read::ZipArchive;

use std::env;
use std::fs::File;
use std::io;
use std::io::Cursor;
use std::path::Path;

const BASE_URL: &str = "https://cn.dll-files.com";

const X32_SYSTEM_PATH: &str = r"C:\Windows\SysWOW64\";
const X64_SYSTEM_PATH: &str = r"C:\Windows\System32\";

fn main() -> anyhow::Result<()> {
    println!("Installing...");

    let dll_name = parse_dll_name().context("Parse dll name fail")?;

    let dll = Dll::new(dll_name);

    let (x32_page_url, x64_page_url) = dll
        .get_downpage_url()
        .context(format!("Get the {} download page url fail", dll.name))?;

    if x32_page_url.is_empty() {
        println!("The x32 {} download page url not found", dll.name);
    } else {
        let download_url = dll
            .get_download_url(&x32_page_url)
            .context(format!("Get the x32 {} download url fail", dll.name))?;
        dll.install_dll(&download_url, Architecture::X32)
            .context(format!("Install the x32 {} fail", dll.name))?;
        println!("Install the x32 {} success!", dll.name);
    }

    if x64_page_url.is_empty() {
        println!("The x64 {} download page url not found", dll.name);
    } else {
        let download_url = dll
            .get_download_url(&x64_page_url)
            .context(format!("Get the x64 {} download url fail", dll.name))?;
        dll.install_dll(&download_url, Architecture::X64)
            .context(format!("Install the x64 {} fail", dll.name))?;
        println!("Install the x64 {} success!", dll.name);
    }

    Ok(())
}

fn parse_dll_name() -> anyhow::Result<String> {
    let mut args = env::args();
    args.next().unwrap();

    let dll = args.next().context("Not enough argument")?.to_lowercase();

    if !dll.ends_with(".dll") {
        anyhow::bail!("The argument must end with .dll");
    }

    Ok(dll)
}

enum Architecture {
    X32,
    X64,
}

struct Dll {
    name: String,
}

impl Dll {
    fn new(name: String) -> Self {
        Self { name }
    }

    fn get_downpage_url(&self) -> anyhow::Result<(String, String)> {
        let resp = minreq::get(format!("{BASE_URL}/{}.html", self.name))
            .send()
            .context("Send request fail")?;

        let html = resp.as_str().context("Read response fail")?;
        if html.contains("error-404") {
            anyhow::bail!("The dll html page not found");
        }

        let sections = Regex::new(r#"(?s)<section class="file-info-grid".+?</section>"#)
            .unwrap()
            .find_iter(html)
            .map(|m| m.as_str())
            .collect::<Vec<&str>>();

        let mut x32_url = "".to_string();
        let mut x64_url = "".to_string();

        for &section in &sections {
            if !x32_url.is_empty() && !x64_url.is_empty() {
                break;
            }
            let meta_info = Regex::new(r#"(?s)<div\sclass="right-pane".+?</div>"#)?
                .find(section)
                .map(|m| m.as_str())
                .unwrap_or("");
            if meta_info.is_empty() {
                continue;
            }
            let architecture = Regex::new(r#"(?s)<p>(?<arch>.+?)</p>"#)?
                .captures_iter(meta_info)
                .map(|m| m.name("arch").unwrap().as_str())
                .nth(1)
                .unwrap_or("");
            if architecture.is_empty() {
                continue;
            }
            if architecture == "32" && !x32_url.is_empty()
                || architecture == "64" && !x64_url.is_empty()
            {
                continue;
            }
            let downlink = Regex::new(r#"(?s)<a href="(?<link>.+?)"\sdata-ga-action"#)?
                .captures(section)
                .map(|m| m.name("link").unwrap().as_str())
                .unwrap_or("");
            if downlink.is_empty() {
                continue;
            }
            if architecture == "32" {
                x32_url = format!("{BASE_URL}{downlink}");
            }
            if architecture == "64" {
                x64_url = format!("{BASE_URL}{downlink}");
            }
        }

        Ok((x32_url, x64_url))
    }

    fn get_download_url(&self, downpage_url: &str) -> anyhow::Result<String> {
        let resp = minreq::get(downpage_url)
            .send()
            .context("send request fail")?;

        let html = resp.as_str().context("read response fail")?;

        let url = Regex::new(r#"downloadUrl\s=\s"(?<link>.+?)";"#)?
            .captures(html)
            .map(|m| m.name("link").unwrap().as_str().replace("amp;", ""));

        if url.is_none() {
            anyhow::bail!("The dll download url not found");
        }

        Ok(url.unwrap())
    }

    fn install_dll(&self, download_url: &str, arch: Architecture) -> anyhow::Result<()> {
        let resp = minreq::get(download_url)
            .send()
            .context("send request fail")?;

        let dll_zip = resp.as_bytes();

        let cursor = Cursor::new(dll_zip);
        let mut archive = ZipArchive::new(cursor).context("New zip archive fail")?;

        for i in 0..archive.len() {
            let mut file = archive.by_index(i).context("Get the contained file fail")?;
            if !file.name().ends_with(".dll") {
                continue;
            }
            let dll_file_path = match arch {
                Architecture::X32 => format!("{}{}", X32_SYSTEM_PATH, self.name),
                Architecture::X64 => format!("{}{}", X64_SYSTEM_PATH, self.name),
            };
            if Path::new(&dll_file_path).exists() {
                continue;
            }
            let mut dll_file = File::create(dll_file_path).context("Create the dll file fail")?;

            io::copy(&mut file, &mut dll_file).context("Write the dll file fail")?;
        }

        Ok(())
    }
}
