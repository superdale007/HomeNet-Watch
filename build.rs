// Records the build date so the About page can show which build you're running.
fn main() {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0) as i64;
    // days since 1970-01-01 -> year-month-day (civil calendar)
    let z = secs / 86400 + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
    println!("cargo:rustc-env=HNW_BUILD_DATE={:04}-{:02}-{:02}", y, m, d);
    println!("cargo:rerun-if-changed=src");
    // Published builds get the GitHub repository name for in-app updates.
    println!("cargo:rerun-if-env-changed=HNW_UPDATE_REPO");
}
