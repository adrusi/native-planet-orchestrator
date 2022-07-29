use anyhow::Result;
use async_trait::async_trait;
use log::error;

#[async_trait]
pub trait AsyncDrop {
    async fn async_drop_result(&mut self) -> Result<()> {
        Ok(())
    }

    async fn async_drop(&mut self) {
        match self.async_drop_result().await {
            Ok(_) => {},
            Err(err) => {
                error!("error encountered in async_drop(): {}", err);
            }
        }
    }
}