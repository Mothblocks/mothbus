use std::{
    future::Future,
    pin::Pin,
    sync::{atomic::AtomicBool, Arc},
    time::{Duration, Instant},
};

use tokio::sync::RwLock as TokioRwLock;

use crate::{hide_debug::HideDebug, State};

pub trait ReservedCacheGeneratorOutput: Send + Sync {
    type Output;
    fn call(
        &self,
        state: Arc<State>,
    ) -> Pin<Box<dyn Future<Output = color_eyre::Result<Self::Output>> + Send + 'static>>;
}

impl<F, Ft, O> ReservedCacheGeneratorOutput for F
where
    F: Fn(Arc<State>) -> Ft + Send + Sync,
    Ft: Future<Output = color_eyre::Result<O>> + Send + 'static,
{
    type Output = O;

    fn call(
        &self,
        state: Arc<State>,
    ) -> Pin<Box<dyn Future<Output = color_eyre::Result<Self::Output>> + Send + 'static>> {
        Box::pin((self)(state))
    }
}

// Caches T and will regenerate it if the cache is empty,
// but will still give back the old value no matter what.
#[derive(Debug)]
pub struct ReservedCache<T: Send + Sync + 'static> {
    cache: TokioRwLock<Option<(Instant, Arc<T>)>>,
    writing: AtomicBool,
    generator: HideDebug<Box<dyn ReservedCacheGeneratorOutput<Output = T>>>,
    ttl: Duration,
}

impl<T: Send + Sync + 'static> ReservedCache<T> {
    pub fn new(
        ttl: Duration,
        generator: impl ReservedCacheGeneratorOutput<Output = T> + 'static,
    ) -> Self {
        Self {
            cache: Default::default(),
            writing: AtomicBool::new(false),
            generator: HideDebug(Box::new(generator)),
            ttl,
        }
    }

    pub async fn get(self: Arc<Self>, state: Arc<State>) -> color_eyre::Result<Arc<T>> {
        let read = self.cache.read().await;
        match &*read {
            Some((instant, value)) if instant.elapsed() < self.ttl => Ok(Arc::clone(value)),
            Some((_, value)) => {
                let value = Arc::clone(value);

                if !self.writing.swap(true, std::sync::atomic::Ordering::SeqCst) {
                    drop(read);
                    self.start_write(state);
                }

                Ok(value)
            }
            None => {
                drop(read);

                let mut write = self.cache.write().await;
                match &*write {
                    Some((_, value)) => Ok(Arc::clone(value)), // data race memes?
                    None => {
                        let value = Arc::new(self.generator.call(state).await?);
                        *write = Some((Instant::now(), Arc::clone(&value)));
                        Ok(value)
                    }
                }
            }
        }
    }

    // #[cfg(test)]
    // pub async fn end_ttl(&self) {
    //     let mut write = self.cache.write().await;
    //     if let Some((instant, _)) = &mut *write {
    //         *instant = Instant::now() - self.ttl;
    //     }
    // }

    fn start_write(self: Arc<Self>, state: Arc<State>) {
        let this = Arc::clone(&self);

        tokio::task::spawn(async move {
            let value = Arc::new(this.generator.call(state).await?);
            let mut write = this.cache.write().await;
            *write = Some((Instant::now(), Arc::clone(&value)));
            this.writing
                .store(false, std::sync::atomic::Ordering::SeqCst);
            Ok::<_, color_eyre::Report>(())
        });
    }
}

/*
#[cfg(test)]
mod test {
    use once_cell::sync::Lazy;

    use super::*;

    static NOW: Lazy<Instant> = Lazy::new(Instant::now);

    #[tokio::test]
    async fn reserved_cache() {
        let cache = Arc::new(ReservedCache::new(Duration::from_secs(10), || async {
            Ok(NOW.elapsed().as_nanos())
        }));

        let value = Arc::clone(&cache).get().await.unwrap();
        let value2 = Arc::clone(&cache).get().await.unwrap();
        assert_eq!(value, value2);

        cache.end_ttl().await;

        let value3 = Arc::clone(&cache).get().await.unwrap();
        assert_eq!(value, value3);

        tokio::time::sleep(Duration::from_millis(10)).await;

        let value4 = Arc::clone(&cache).get().await.unwrap();
        assert_ne!(value, value4);
    }
}
*/
