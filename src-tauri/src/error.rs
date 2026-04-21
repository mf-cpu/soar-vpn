use serde::{Serialize, Serializer};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("IO 错误: {0}")]
    Io(#[from] std::io::Error),

    #[error("命令执行失败: {0}")]
    Command(String),

    #[error("配置不存在: {0}")]
    ConfigNotFound(String),

    #[error("配置无效: {0}")]
    InvalidConfig(String),

    #[error("用户取消授权")]
    UserCancelled,

    #[error("{0}")]
    Other(String),
}

impl Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.to_string().as_ref())
    }
}

pub type AppResult<T> = Result<T, AppError>;
