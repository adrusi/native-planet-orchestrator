#[allow(unused_imports)] use crate::prelude::*;

use actix_web::web::Bytes;
use futures::{ready, Stream};
use futures::stream;
use generic_array::GenericArray;
use pin_project_lite::pin_project;
use std::task::Poll;
use sha2::Digest;

#[async_trait]
pub trait AsyncDrop {
    async fn async_drop_result(&mut self) -> Result<()> {
        Ok(())
    }

    async fn async_drop(&mut self) {
        match self.async_drop_result().await {
            Ok(_) => {},
            Err(err) => {
                log::error!("error encountered in async_drop(): {}", err);
            }
        }
    }
}

pub trait MyStreamExt : Stream {
    fn into_checksum_verify<D: Digest>(
        self,
        checksum: GenericArray<u8, D::OutputSize>
    ) -> ChecksumVerifyStream<Self, D>
        where Self: Sized;
}

impl<S: Stream> MyStreamExt for S {
    fn into_checksum_verify<D: Digest>(
        self,
        checksum: GenericArray<u8, D::OutputSize>
    ) -> ChecksumVerifyStream<Self, D>
        where Self: Sized
    {
        ChecksumVerifyStream::new(self, checksum)
    }
}

pub trait IntoResultAsRefBytes {
    type Item: AsRef<[u8]>;
    fn into_result_asref_bytes(self) -> Result<Self::Item>;
}

impl IntoResultAsRefBytes for u8 {
    type Item = [u8; 1];
    fn into_result_asref_bytes(self) -> Result<Self::Item> {
        Ok([self])
    }
}

impl IntoResultAsRefBytes for Bytes {
    type Item = Self;
    fn into_result_asref_bytes(self) -> Result<Self::Item> {
        Ok(self)
    }
}

impl<A: IntoResultAsRefBytes, E> IntoResultAsRefBytes for std::result::Result<A, E>
    where E: 'static + Send + Sync + std::error::Error
{
    type Item = A::Item;
    fn into_result_asref_bytes(self) -> Result<Self::Item> {
        let byte = self?;
        byte.into_result_asref_bytes()
    }
}

pin_project! {
    #[derive(Debug)]
    #[must_use = "streams do nothing unless polled"]
    pub struct ChecksumVerifyStream<Src, D: Digest> {
        #[pin]
        src: stream::Fuse<Src>,
        digest: Option<D>,
        checksum: GenericArray<u8, D::OutputSize>,
    }
}

impl<Src: Stream, D: Digest> ChecksumVerifyStream<Src, D> {
    fn new(src: Src, checksum: GenericArray<u8, D::OutputSize>) -> Self {
        Self {
            src: src.fuse(),
            digest: Some(D::new()),
            checksum,
        }
    }
}

impl<A: IntoResultAsRefBytes, Src: Unpin + Stream<Item = A>, D: Digest> Stream for ChecksumVerifyStream<Src, D> {
    type Item = Result<A::Item>;

    fn poll_next(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>
    ) -> Poll<Option<Self::Item>> {
        let mut this = self.project();

        let digest = match this.digest.as_mut() {
            Some(d) => d,
            None => return Poll::Ready(None),
        };

        let res = ready!(this.src.as_mut().poll_next(cx));

        Poll::Ready(match res {
            Some(b) => {
                match b.into_result_asref_bytes() {
                    Err(e) => Some(Err(e)),
                    Ok(bytes) => {
                        digest.update(bytes.as_ref());
                        Some(Ok(bytes))
                    },
                }
            },
            None => {
                let actual_checksum = this.digest.take().unwrap().finalize();
                if &actual_checksum[..] == &this.checksum[..] {
                    None
                } else {
                    Some(Err(anyhow!("checksum validation failed")))
                }
            },
        })
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.src.size_hint()
    }
}