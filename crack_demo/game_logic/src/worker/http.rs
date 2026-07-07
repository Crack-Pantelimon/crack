#[cfg(not(target_family = "wasm"))]
pub async fn http_get_bytes(url: &str) -> anyhow::Result<bytes::Bytes> {
    let resp = reqwest::get(url).await?.error_for_status()?;
    Ok(resp.bytes().await?)
}

#[cfg(not(target_family = "wasm"))]
pub async fn http_get_text(url: &str) -> anyhow::Result<String> {
    let resp = reqwest::get(url).await?.error_for_status()?;
    Ok(resp.text().await?)
}

#[cfg(target_family = "wasm")]
pub async fn http_get_bytes(url: &str) -> anyhow::Result<bytes::Bytes> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let url = url.to_string();
    wasm_bindgen_futures::spawn_local(async move {
        let res = async {
            let resp = reqwest::get(&url).await?.error_for_status()?;
            let bytes = resp.bytes().await?;
            Ok::<_, anyhow::Error>(bytes)
        }
        .await;
        let _ = tx.send(res);
    });
    rx.await?
}

#[cfg(target_family = "wasm")]
pub async fn http_get_text(url: &str) -> anyhow::Result<String> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let url = url.to_string();
    wasm_bindgen_futures::spawn_local(async move {
        let res = async {
            let resp = reqwest::get(&url).await?.error_for_status()?;
            let text = resp.text().await?;
            Ok::<_, anyhow::Error>(text)
        }
        .await;
        let _ = tx.send(res);
    });
    rx.await?
}
