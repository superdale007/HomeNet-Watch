#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod backup;
mod categories;
mod report;
mod sha256;
mod sysnet;
mod traffic;
mod trust;
mod update;
mod util;
mod worldmap;
mod checks;
mod model;
mod perf;
mod secret;
mod snmp;
mod snmpv3;
#[cfg(not(windows))]
mod unix;
mod web;
mod monitor;
mod net;
mod theme;
mod widgets;

use eframe::egui;
use std::path::PathBuf;

fn main() -> eframe::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let mut screenshot: Option<PathBuf> = None;
    let mut page: Option<String> = None;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--screenshot" if i + 1 < args.len() => {
                screenshot = Some(PathBuf::from(&args[i + 1]));
                i += 1;
            }
            "--page" if i + 1 < args.len() => {
                page = Some(args[i + 1].clone());
                i += 1;
            }
            // Troubleshooting: print what a device reports over SNMP, then exit.
            //   --snmp-probe <ip> [community]
            //   --snmp-probe <ip> v3 <user> <MD5|SHA1|SHA256|none> <auth password> <DES|AES|none> <privacy password>
            "--snmp-probe" if i + 1 < args.len() => {
                let ip: std::net::Ipv4Addr = args[i + 1].parse().expect("address");
                let arg = |n: usize| args.get(i + n).cloned().unwrap_or_default();
                let sec = if arg(2) == "v3" {
                    use snmpv3::{AuthProto, PrivProto};
                    let auth = match arg(4).to_uppercase().as_str() {
                        "MD5" => AuthProto::Md5,
                        "SHA" | "SHA1" | "SHA-1" => AuthProto::Sha1,
                        "SHA256" | "SHA-256" => AuthProto::Sha256,
                        _ => AuthProto::None,
                    };
                    let privp = match arg(6).to_uppercase().as_str() {
                        "DES" => PrivProto::Des,
                        "AES" | "AES128" | "AES-128" => PrivProto::Aes128,
                        _ => PrivProto::None,
                    };
                    snmp::Security::V3 { user: arg(3), auth, auth_pass: arg(5), privp, priv_pass: arg(7) }
                } else {
                    snmp::Security::V2c(if arg(2).is_empty() { "public".into() } else { arg(2) })
                };
                let first = perf::collect(ip, &sec, &Default::default());
                std::thread::sleep(std::time::Duration::from_secs(3));
                let prev = first.as_ref().map(|r| r.1.clone()).unwrap_or_default();
                match perf::collect(ip, &sec, &prev) {
                    Ok((p, _)) => println!("{:#?}", p),
                    Err(e) => println!("error: {}", e),
                }
                std::process::exit(0);
            }
            _ => {}
        }
        i += 1;
    }

    let mut viewport = egui::ViewportBuilder::default()
        .with_title("HomeNet Watch by DDID")
        .with_inner_size([1360.0, 860.0])
        .with_min_inner_size([980.0, 680.0]);
    if screenshot.is_some() {
        viewport = viewport.with_active(false);
        #[cfg(windows)]
        {
            viewport = viewport.with_window_level(egui::WindowLevel::AlwaysOnBottom);
        }
    }

    #[allow(unused_mut)]
    let mut options = eframe::NativeOptions { viewport, ..Default::default() };
    #[cfg(target_os = "macos")]
    if screenshot.is_some() {
        options.event_loop_builder = Some(Box::new(|b| {
            use winit::platform::macos::EventLoopBuilderExtMacOS;
            b.with_activate_ignoring_other_apps(false);
        }));
    }

    eframe::run_native(
        "HomeNet Watch by DDID",
        options,
        Box::new(move |cc| Ok(Box::new(app::App::new(cc, screenshot, page)))),
    )
}
