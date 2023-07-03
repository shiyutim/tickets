use reqwest::{header, Client};

#[tokio::main]
async fn get_repo_release() -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let client = Client::builder().build()?;
    let mut headers = header::HeaderMap::new();
    headers.insert("user-agent", "Mozilla/5.0 (Linux; Android 6.0; Nexus 5 Build/MRA58N) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/113.0.0.0 Mobile Safari/537.36".parse().unwrap());
    let res = client
        .get("https://api.github.com/repos/shiyutim/tickets/releases")
        .headers(headers)
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?;

    Ok(res)
}

#[tauri::command]
pub fn get_repo_version() -> String {
    let res = get_repo_release();
    match res {
        Ok(s) => {
            if let Some(arr) = s.as_array() {
                let first = arr.get(0);
                match first {
                    Some(f) => {
                        let tag_name = f["tag_name"].to_string();
                        tag_name.replace("v", "")
                    }
                    None => String::new(),
                }
            } else {
                String::new()
            }
        }
        Err(e) => {
            println!("error : {}", e);
            String::new()
        }
    }
}
