use anyhow::{Context, Result};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};

const HEADER_CONTENT_LENGTH: &str = "content-length";

pub async fn read_frame(
    reader: &mut BufReader<tokio::io::Stdin>,
    max_frame_bytes: usize,
) -> Result<Option<Vec<u8>>> {
    let mut line = String::new();
    let mut content_length: Option<usize> = None;

    loop {
        line.clear();
        let bytes = reader.read_line(&mut line).await?;
        if bytes == 0 {
            return Ok(None);
        }

        if line == "\r\n" || line == "\n" {
            break;
        }

        let Some((k, v)) = line.split_once(':') else {
            continue;
        };
        if k.trim().eq_ignore_ascii_case(HEADER_CONTENT_LENGTH) {
            let parsed = v
                .trim()
                .parse::<usize>()
                .context("Invalid Content-Length")?;
            if parsed > max_frame_bytes {
                anyhow::bail!("Frame too large: {} > {}", parsed, max_frame_bytes);
            }
            content_length = Some(parsed);
        }
    }

    let len = content_length.context("Missing Content-Length header")?;
    let mut payload = vec![0_u8; len];
    reader.read_exact(&mut payload).await?;
    Ok(Some(payload))
}

pub async fn write_frame(writer: &mut tokio::io::Stdout, payload: &[u8]) -> Result<()> {
    writer
        .write_all(format!("Content-Length: {}\r\n", payload.len()).as_bytes())
        .await?;
    writer
        .write_all(b"Content-Type: application/json\r\n\r\n")
        .await?;
    writer.write_all(payload).await?;
    writer.flush().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn content_length_header_case_insensitive() {
        assert!("Content-Length".eq_ignore_ascii_case("content-length"));
    }
}
