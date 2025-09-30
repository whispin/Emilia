use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{self, BufRead, BufReader, Write}; // Read dihapus karena AsyncReadExt akan digunakan
use std::net::IpAddr;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures::StreamExt;
use ipnetwork::IpNetwork;
use maxminddb::{geoip2, Reader};
use native_tls::TlsConnector as NativeTlsConnector; // Renamed to avoid conflict
use serde_json::Value;
use tokio::io::{AsyncReadExt, AsyncWriteExt}; // Untuk read_exact, write_all async
use tokio::net::TcpStream; // TcpStream async dari Tokio
use tokio_native_tls::TlsConnector as TokioTlsConnector; // Konektor TLS async

const IP_RESOLVER: &str = "speed.cloudflare.com";
const PATH_RESOLVER: &str = "/meta";
const PROXY_FILE: &str = "Data/emeliaProxyIP15AGS.txt"; //input
const OUTPUT_FILE: &str = "Data/alive.txt";
const COUNTRY_DB: &str = "Data/GeoLite2-Country.mmdb";
const CITY_DB: &str = "Data/GeoLite2-City.mmdb";
const ANONYMOUS_IP_DB: &str = "Data/GeoIP2-Anonymous-IP.mmdb";
const ABUSE_IP_FILE: &str = "Data/abuseips.txt";
const FIREHOL_CIDR_FILE: &str = "Data/firehol_cidr.txt";
const MAX_CONCURRENT: usize = 175;
const TIMEOUT_SECONDS: u64 = 9;

// Define a custom error type that implements Send + SData/ProxyIP2Agustus.txtync
type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

#[tokio::main]
async fn main() -> Result<()> {
    println!("Starting proxy scanner...");

    // Create output directory if it doesn't exist
    if let Some(parent) = Path::new(OUTPUT_FILE).parent() {
        fs::create_dir_all(parent)?;
    }

    // Initialize GeoIP database readers
    let country_reader = Arc::new(Reader::open_readfile(COUNTRY_DB)?);
    println!("Loaded Country database: {}", COUNTRY_DB);

    let city_reader = match Reader::open_readfile(CITY_DB) {
        Ok(reader) => {
            println!("Loaded City database: {}", CITY_DB);
            Some(Arc::new(reader))
        }
        Err(e) => {
            eprintln!("Warning: Could not load City database ({}): {}. City info will show as '未知'.", CITY_DB, e);
            None
        }
    };

    // Initialize Anonymous IP database reader (optional)
    let anonymous_reader = match Reader::open_readfile(ANONYMOUS_IP_DB) {
        Ok(reader) => {
            println!("Loaded Anonymous IP database: {}", ANONYMOUS_IP_DB);
            Some(Arc::new(reader))
        }
        Err(e) => {
            eprintln!("Warning: Could not load Anonymous IP database ({}): {}. Anonymous IP filtering will be disabled.", ANONYMOUS_IP_DB, e);
            None
        }
    };

    // Load AbuseIPDB blacklist
    let abuse_ips = Arc::new(load_abuse_ips(ABUSE_IP_FILE));

    // Load FireHOL CIDR blocklist
    let firehol_cidrs = Arc::new(load_firehol_cidrs(FIREHOL_CIDR_FILE));

    // Clear output file before starting
    // File::create akan mengosongkan file jika sudah ada atau membuatnya jika belum
    File::create(OUTPUT_FILE)?;
    println!("File {} has been cleared or created before scanning process started.", OUTPUT_FILE);

    // Read proxy list from file
    let proxies = match read_proxy_file(PROXY_FILE) {
        Ok(proxies) => proxies,
        Err(e) => {
            eprintln!("Error reading proxy file: {}", e);
            return Err(e.into());
        }
    };

    println!("Loaded {} proxies from file", proxies.len());

    // Get original IP (without proxy)
    let original_ip_data = match check_connection(IP_RESOLVER, PATH_RESOLVER, None).await {
        Ok(data) => data,
        Err(e) => {
            eprintln!("Failed to get original IP info: {}", e);
            // Consider if you want to exit here. If speed.cloudflare.com is down, no checks can be done.
            return Err(e.into());
        }
    };

    let original_ip = match original_ip_data.get("clientIp") {
        Some(Value::String(ip)) => ip.clone(),
        _ => {
            eprintln!("Failed to extract original client IP from response: {:?}", original_ip_data);
            return Err("Failed to extract original client IP".into());
        }
    };

    println!("Original IP: {}", original_ip);

    // Store active proxies
    let active_proxies = Arc::new(Mutex::new(Vec::new()));

    // Process proxies concurrently
    let tasks = futures::stream::iter(
        proxies.into_iter().map(|proxy_line| {
            let original_ip = original_ip.clone();
            let active_proxies = Arc::clone(&active_proxies);
            let country_reader = Arc::clone(&country_reader);
            let city_reader = city_reader.clone();
            let anonymous_reader = anonymous_reader.clone();
            let abuse_ips = Arc::clone(&abuse_ips);
            let firehol_cidrs = Arc::clone(&firehol_cidrs);

            // tokio::spawn akan menjalankan setiap future process_proxy secara independen
            // Ini adalah cara yang lebih idiomatik untuk menjalankan banyak tugas async di Tokio
            // daripada hanya mengandalkan buffer_unordered pada stream dari async blok.
            // Namun, karena buffer_unordered sudah menangani konkurensi,
            // tokio::spawn di sini mungkin redundan jika process_proxy itu sendiri tidak
            // melakukan spawn lebih lanjut atau operasi berat CPU yang panjang.
            // Untuk I/O bound seperti ini, buffer_unordered sudah cukup.
            // Mari kita tetap dengan struktur asli untuk kesederhanaan, karena buffer_unordered sudah menangani konkurensi.
            async move {
                process_proxy(
                    proxy_line,
                    &original_ip,
                    &active_proxies,
                    &country_reader,
                    city_reader.as_deref(),
                    anonymous_reader.as_deref(),
                    &abuse_ips,
                    &firehol_cidrs,
                ).await;
            }
        })
    ).buffer_unordered(MAX_CONCURRENT).collect::<Vec<()>>();

    tasks.await;

    // Save active proxies to file
    let active_proxies_locked = active_proxies.lock().unwrap(); // Renamed for clarity
    if !active_proxies_locked.is_empty() {
        let mut file = File::create(OUTPUT_FILE)?; // Buka lagi untuk menulis, ini akan menimpa
        for proxy in active_proxies_locked.iter() {
            writeln!(file, "{}", proxy)?;
        }
        println!("All active proxies saved to {}", OUTPUT_FILE);
    } else {
        println!("No active proxies found");
    }

    println!("Proxy checking completed.");
    Ok(())
}

fn read_proxy_file(file_path: &str) -> io::Result<Vec<String>> {
    let file = File::open(file_path)?;
    let reader = BufReader::new(file);
    let mut proxies = Vec::new();

    for line in reader.lines() {
        let line = line?;
        if !line.trim().is_empty() {
            proxies.push(line);
        }
    }

    Ok(proxies)
}

// 读取 AbuseIPDB 黑名单 IP 列表
fn load_abuse_ips(file_path: &str) -> HashSet<IpAddr> {
    let mut abuse_ips = HashSet::new();

    match File::open(file_path) {
        Ok(file) => {
            let reader = BufReader::new(file);
            for line in reader.lines() {
                if let Ok(line) = line {
                    // 格式: ip,country_code,abuse_confidence_score
                    let parts: Vec<&str> = line.split(',').collect();
                    if !parts.is_empty() {
                        if let Ok(ip) = parts[0].trim().parse::<IpAddr>() {
                            abuse_ips.insert(ip);
                        }
                    }
                }
            }
            println!("Loaded {} abuse IPs from {}", abuse_ips.len(), file_path);
        }
        Err(e) => {
            eprintln!("Warning: Could not load abuse IP list ({}): {}. Abuse IP filtering will be disabled.", file_path, e);
        }
    }

    abuse_ips
}

// 读取 FireHOL CIDR 网段列表
fn load_firehol_cidrs(file_path: &str) -> Vec<IpNetwork> {
    let mut cidrs = Vec::new();

    match File::open(file_path) {
        Ok(file) => {
            let reader = BufReader::new(file);
            for line in reader.lines() {
                if let Ok(line) = line {
                    let line = line.trim();
                    if !line.is_empty() {
                        if let Ok(network) = line.parse::<IpNetwork>() {
                            cidrs.push(network);
                        }
                    }
                }
            }
            println!("Loaded {} CIDR ranges from {}", cidrs.len(), file_path);
        }
        Err(e) => {
            eprintln!("Warning: Could not load FireHOL CIDR list ({}): {}. CIDR filtering will be disabled.", file_path, e);
        }
    }

    cidrs
}

// 检查 IP 是否在 CIDR 网段内
fn is_ip_in_cidr_list(ip: IpAddr, cidrs: &[IpNetwork]) -> bool {
    cidrs.iter().any(|network| network.contains(ip))
}

async fn check_connection(
    host: &str,
    path: &str,
    proxy: Option<(&str, u16)>,
) -> Result<Value> {
    let timeout_duration = Duration::from_secs(TIMEOUT_SECONDS);

    // Bungkus seluruh operasi koneksi dalam tokio::time::timeout
    match tokio::time::timeout(timeout_duration, async {
        // Build HTTP request payload
        let payload = format!(
            "GET {} HTTP/1.1\r\n\
             Host: {}\r\n\
             User-Agent: Mozilla/5.0 (Windows NT 10.0) AppleWebKit/537.36 \
             (KHTML, like Gecko) Chrome/42.0.2311.135 Safari/537.36 Edge/12.10240\r\n\
             Connection: close\r\n\r\n",
            path, host
        );

        // Create TCP connection
        let stream = if let Some((proxy_ip, proxy_port)) = proxy {
            // *** PERUBAHAN UTAMA DI SINI ***
            // Menangani alamat IPv6 dengan benar dengan membungkusnya dalam kurung siku.
            let connect_addr = if proxy_ip.contains(':') {
                // Ini adalah alamat IPv6, formatnya menjadi "[ipv6]:port"
                format!("[{}]:{}", proxy_ip, proxy_port)
            } else {
                // Ini adalah alamat IPv4, formatnya tetap "ipv4:port"
                format!("{}:{}", proxy_ip, proxy_port)
            };
            TcpStream::connect(connect_addr).await?
        } else {
            // Connect directly to host (Tokio's connect can resolve hostnames)
            TcpStream::connect(format!("{}:443", host)).await?
        };

        // Create TLS connection
        // NativeTlsConnector dikonfigurasi terlebih dahulu
        let native_connector = NativeTlsConnector::builder().build()?;
        // Kemudian dibungkus dengan TokioTlsConnector untuk penggunaan async
        let tokio_connector = TokioTlsConnector::from(native_connector);

        let mut tls_stream = tokio_connector.connect(host, stream).await?;

        // Send HTTP request
        tls_stream.write_all(payload.as_bytes()).await?;

        // Read response
        let mut response = Vec::new();
        // Menggunakan buffer yang sama ukurannya
        let mut buffer = [0; 4096];

        // Loop untuk membaca data dari stream
        // AsyncReadExt::read akan mengembalikan Ok(0) saat EOF.
        loop {
            match tls_stream.read(&mut buffer).await {
                Ok(0) => break, // End of stream
                Ok(n) => response.extend_from_slice(&buffer[..n]),
                Err(e) => {
                    // Jika jenis error adalah WouldBlock, dalam konteks async,
                    // ini biasanya ditangani oleh runtime (tidak akan sampai ke sini jika .await digunakan dengan benar).
                    // Namun, jika ada error I/O lain, kita return.
                    return Err(Box::new(e) as Box<dyn std::error::Error + Send + Sync>);
                }
            }
        }

        // Parse response
        let response_str = String::from_utf8_lossy(&response);

        // Split headers and body
        if let Some(body_start) = response_str.find("\r\n\r\n") {
            let body = &response_str[body_start + 4..];

            // Try to parse the JSON body
            match serde_json::from_str::<Value>(body.trim()) {
                Ok(json_data) => Ok(json_data),
                Err(e) => {
                    eprintln!("Failed to parse JSON: {}", e);
                    eprintln!("Response body for {}:{}: {}", host, proxy.map_or_else(|| "direct".to_string(), |(ip,p)| format!("{}:{}",ip,p)), body);
                    Err("Invalid JSON response".into())
                }
            }
        } else {
            Err("Invalid HTTP response: No separator found".into())
        }
    }).await {
        Ok(inner_result) => inner_result, // Hasil dari blok async (bisa Ok atau Err)
        Err(_) => Err(Box::new(io::Error::new(io::ErrorKind::TimedOut, "Connection attempt timed out")) as Box<dyn std::error::Error + Send + Sync>), // Error karena timeout
    }
}


fn clean_org_name(org_name: &str) -> String {
    org_name.chars()
        .filter(|c| c.is_alphanumeric() || c.is_whitespace())
        .collect()
}

// 查询 IP 地理位置信息
fn get_geo_info(
    country_reader: &Reader<Vec<u8>>,
    city_reader: Option<&Reader<Vec<u8>>>,
    ip_str: &str,
) -> (String, String) {
    let ip: IpAddr = match ip_str.parse() {
        Ok(ip) => ip,
        Err(_) => return ("未知".to_string(), "未知".to_string()),
    };

    // 查询国家信息
    let country = match country_reader.lookup::<geoip2::Country>(ip) {
        Ok(country_data) => {
            country_data
                .country
                .and_then(|c| c.names)
                .and_then(|names| {
                    names.get("zh-CN")
                        .or_else(|| names.get("en"))
                        .map(|s| s.to_string())
                })
                .unwrap_or_else(|| "未知".to_string())
        }
        Err(_) => "未知".to_string(),
    };

    // 查询城市信息（如果有城市数据库）
    let city = if let Some(reader) = city_reader {
        match reader.lookup::<geoip2::City>(ip) {
            Ok(city_data) => {
                city_data
                    .city
                    .and_then(|c| c.names)
                    .and_then(|names| {
                        names.get("zh-CN")
                            .or_else(|| names.get("en"))
                            .map(|s| s.to_string())
                    })
                    .unwrap_or_else(|| "未知".to_string())
            }
            Err(_) => "未知".to_string(),
        }
    } else {
        "未知".to_string()
    };

    (country, city)
}

// 检查 IP 是否为匿名代理（VPN/公共代理/Tor）
fn is_anonymous_ip(
    anonymous_reader: &Reader<Vec<u8>>,
    ip_str: &str,
) -> (bool, String) {
    let ip: IpAddr = match ip_str.parse() {
        Ok(ip) => ip,
        Err(_) => return (false, "无法解析IP".to_string()),
    };

    match anonymous_reader.lookup::<geoip2::AnonymousIp>(ip) {
        Ok(anonymous_data) => {
            let is_vpn = anonymous_data.is_anonymous_vpn.unwrap_or(false);
            let is_proxy = anonymous_data.is_public_proxy.unwrap_or(false);
            let is_tor = anonymous_data.is_tor_exit_node.unwrap_or(false);

            if is_vpn || is_proxy || is_tor {
                let mut reasons = Vec::new();
                if is_vpn { reasons.push("VPN"); }
                if is_proxy { reasons.push("公共代理"); }
                if is_tor { reasons.push("Tor出口节点"); }
                (true, reasons.join("+"))
            } else {
                (false, "正常IP".to_string())
            }
        }
        Err(_) => {
            // 数据库中没有记录，视为正常IP（不在匿名IP列表中）
            (false, "未知(默认允许)".to_string())
        }
    }
}

async fn process_proxy(
    proxy_line: String,
    original_ip: &str,
    active_proxies: &Arc<Mutex<Vec<String>>>,
    country_reader: &Reader<Vec<u8>>,
    city_reader: Option<&Reader<Vec<u8>>>,
    anonymous_reader: Option<&Reader<Vec<u8>>>,
    abuse_ips: &HashSet<IpAddr>,
    firehol_cidrs: &[IpNetwork],
) {
    let parts: Vec<&str> = proxy_line.split(',').collect();
    if parts.len() < 4 {
        println!("Invalid proxy line format: {}. Expected ip,port,country,org", proxy_line);
        return;
    }

    let ip = parts[0];
    let port_str = parts[1]; // Renamed to avoid conflict with port_num
    let _country = parts[2]; // 保留以备将来使用
    let _org = parts[3]; // 保留以备将来使用

    let port_num = match port_str.parse::<u16>() {
        Ok(p) => p,
        Err(_) => {
            println!("Invalid port number: {} in line: {}", port_str, proxy_line);
            return;
        }
    };

    match check_connection(IP_RESOLVER, PATH_RESOLVER, Some((ip, port_num))).await {
        Ok(proxy_data) => {
            if let Some(Value::String(proxy_ip)) = proxy_data.get("clientIp") {
                if proxy_ip != original_ip {
                    // 解析 IP 地址用于过滤检查
                    let ip_addr = match ip.parse::<IpAddr>() {
                        Ok(addr) => addr,
                        Err(_) => {
                            println!("CF PROXY FILTERED 🚫 (Invalid IP format): {}:{}", ip, port_num);
                            return;
                        }
                    };

                    // 检查是否为匿名IP（VPN/公共代理/Tor）- 仅当数据库可用时
                    if let Some(anon_reader) = anonymous_reader {
                        let (is_anonymous, reason) = is_anonymous_ip(anon_reader, ip);

                        if is_anonymous {
                            println!("CF PROXY FILTERED 🚫 (匿名IP: {}): {}:{}", reason, ip, port_num);
                            return;
                        }
                    }

                    // 检查是否在 AbuseIPDB 黑名单中
                    if !abuse_ips.is_empty() && abuse_ips.contains(&ip_addr) {
                        println!("CF PROXY FILTERED 🚫 (AbuseIPDB 黑名单): {}:{}", ip, port_num);
                        return;
                    }

                    // 检查是否在 FireHOL CIDR 黑名单中
                    if !firehol_cidrs.is_empty() && is_ip_in_cidr_list(ip_addr, firehol_cidrs) {
                        println!("CF PROXY FILTERED 🚫 (FireHOL CIDR 黑名单): {}:{}", ip, port_num);
                        return;
                    }

                    // 获取地理位置信息
                    let (geo_country, geo_city) = get_geo_info(country_reader, city_reader, ip);

                    // 新格式: ip:port#国家名-城市名-ip-port
                    let proxy_entry = format!("{}:{}#{}-{}-{}-{}",
                        ip, port_num,
                        geo_country, geo_city,
                        ip, port_num
                    );
                    println!("CF PROXY LIVE ✅: {}", proxy_entry);

                    let mut active_proxies_locked = active_proxies.lock().unwrap();
                    active_proxies_locked.push(proxy_entry);
                } else {
                    println!("CF PROXY DEAD ❌ (Same IP as original): {}:{}", ip, port_num);
                }
            } else {
                println!("CF PROXY DEAD ❌ (No clientIp field in response): {}:{} - Response: {:?}", ip, port_num, proxy_data);
            }
        },
        Err(e) => {
            println!("CF PROXY DEAD ⏱️ (Error connecting): {}:{} - {}", ip, port_num, e);
        }
    }
}
