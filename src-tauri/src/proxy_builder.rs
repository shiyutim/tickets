use regex::Regex;
use reqwest::{self, Client, Proxy};
use std::net::{IpAddr, Ipv4Addr};

pub struct ProxyBuilder {
    is_proxy: bool,
    address: String,
    ip: IpAddr,
}

impl ProxyBuilder {
    // TODO
    // address 目前只支持 ip格式
    pub fn new(is_proxy: bool, address: String) -> ProxyBuilder {
        let mut res_proxy = is_proxy;
        let default = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));
        let ip = if is_proxy {
            match ProxyBuilder::extract_ip(address.as_str()) {
                Some(s) => s,
                None => {
                    // 手动设置代理为 false
                    res_proxy = false;
                    default
                }
            }
        } else {
            default
        };

        ProxyBuilder {
            is_proxy: res_proxy,
            ip,
            address,
        }
    }

    pub fn check_proxy_is_success(&self, other_ip: IpAddr) -> bool {
        println!("{}  {other_ip}", self.get_ip());
        self.get_ip() == other_ip
    }
    pub fn get_ip(&self) -> IpAddr {
        self.ip
    }
    pub fn get_client(&self) -> Result<Client, reqwest::Error> {
        if self.is_proxy() {
            let proxy = Proxy::all(self.address())?;
            Client::builder().proxy(proxy).build()
        } else {
            Client::builder().build()
        }
    }

    fn is_proxy(&self) -> bool {
        self.is_proxy
    }

    fn address(&self) -> &String {
        &self.address
    }

    pub fn extract_ip(input: &str) -> Option<IpAddr> {
        // 定义 IP 地址的正则表达式
        let res = Regex::new(
            r#"(?i)^(?:socks5|socks4|https?)://([0-9]+\.[0-9]+\.[0-9]+\.[0-9]+):[0-9]+$"#,
        )
        .unwrap();

        // 尝试从输入中提取 IP 地址
        if let Some(captures) = res.captures(input) {
            if let Some(ip) = captures.get(1) {
                return Some(ip.as_str().parse().unwrap());
            }
        }

        None
    }
}
