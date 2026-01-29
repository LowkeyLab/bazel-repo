use repo::UserSaver;
use user::User;

struct Service<T>
where
    T: UserSaver,
{
    repo: T,
}

impl<T> Service<T>
where
    T: UserSaver,
{
    pub fn new(repo: T) -> Self {
        Self { repo }
    }

    pub async fn create_user(&self, discord_id: u64) -> Result<(), repo::Error> {
        let user = User::new(discord_id);
        self.repo.save(user).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
}
