use std::sync::{Arc, Mutex};

use book_smartz_storage::{Observation, ObservationDeliveryError, Observer, SharedObserver, Store};
use sqlx::{PgPool, postgres::PgPoolOptions};
use testcontainers_modules::{
    postgres::Postgres,
    testcontainers::{ContainerAsync, runners::AsyncRunner},
};

pub struct Fixture {
    _container: ContainerAsync<Postgres>,
    pool: PgPool,
}

impl Fixture {
    pub async fn new() -> Self {
        let container = test_images::postgres().await.start().await.unwrap();
        let url = format!(
            "postgres://postgres:postgres@{}:{}/postgres",
            container.get_host().await.unwrap(),
            container.get_host_port_ipv4(5432).await.unwrap()
        );
        let pool = PgPoolOptions::new()
            .max_connections(8)
            .connect(&url)
            .await
            .unwrap();
        Self {
            _container: container,
            pool,
        }
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }
    pub fn store(&self, observer: SharedObserver) -> Store {
        Store::new(self.pool.clone(), observer)
    }
}

#[derive(Default)]
pub struct Recorder(pub Mutex<Vec<Observation>>);
impl Observer for Recorder {
    fn observe(&self, observation: &Observation) -> Result<(), ObservationDeliveryError> {
        self.0.lock().unwrap().push(observation.clone());
        Ok(())
    }
}
pub fn recorder() -> SharedObserver {
    Arc::new(Recorder::default())
}
