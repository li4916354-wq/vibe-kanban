use axum::{
    Router,
    extract::{Query, State},
    response::Json as ResponseJson,
    routing::get,
};
use deployment::Deployment;
use serde::{Deserialize, Serialize};
use services::services::filesystem::{DirectoryEntry, DirectoryListResponse, FilesystemError};
use ts_rs::TS;
use utils::response::ApiResponse;

use crate::{DeploymentImpl, error::ApiError};

#[derive(Debug, Deserialize)]
pub struct ListDirectoryQuery {
    path: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct FileTreeQuery {
    path: String,
    depth: Option<u32>,
}

#[derive(Debug, Deserialize)]
pub struct ReadFileQuery {
    path: String,
}

#[derive(Debug, Serialize, TS)]
pub struct FileTreeResponse {
    pub entries: Vec<FileTreeNode>,
}

#[derive(Debug, Clone, Serialize, TS)]
pub struct FileTreeNode {
    pub name: String,
    pub path: String,
    #[serde(rename = "type")]
    pub node_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub children: Option<Vec<FileTreeNode>>,
}

#[derive(Debug, Serialize, TS)]
pub struct FileContentResponse {
    pub content: String,
    pub path: String,
}

pub async fn list_directory(
    State(deployment): State<DeploymentImpl>,
    Query(query): Query<ListDirectoryQuery>,
) -> Result<ResponseJson<ApiResponse<DirectoryListResponse>>, ApiError> {
    match deployment.filesystem().list_directory(query.path).await {
        Ok(response) => Ok(ResponseJson(ApiResponse::success(response))),
        Err(FilesystemError::DirectoryDoesNotExist) => {
            Ok(ResponseJson(ApiResponse::error("Directory does not exist")))
        }
        Err(FilesystemError::PathIsNotDirectory) => {
            Ok(ResponseJson(ApiResponse::error("Path is not a directory")))
        }
        Err(FilesystemError::Io(e)) => {
            tracing::error!("Failed to read directory: {}", e);
            Ok(ResponseJson(ApiResponse::error(&format!(
                "Failed to read directory: {}",
                e
            ))))
        }
    }
}

pub async fn list_git_repos(
    State(deployment): State<DeploymentImpl>,
    Query(query): Query<ListDirectoryQuery>,
) -> Result<ResponseJson<ApiResponse<Vec<DirectoryEntry>>>, ApiError> {
    let res = if let Some(ref path) = query.path {
        deployment
            .filesystem()
            .list_git_repos(Some(path.clone()), 800, 1200, Some(3))
            .await
    } else {
        deployment
            .filesystem()
            .list_common_git_repos(800, 1200, Some(4))
            .await
    };
    match res {
        Ok(response) => Ok(ResponseJson(ApiResponse::success(response))),
        Err(FilesystemError::DirectoryDoesNotExist) => {
            Ok(ResponseJson(ApiResponse::error("Directory does not exist")))
        }
        Err(FilesystemError::PathIsNotDirectory) => {
            Ok(ResponseJson(ApiResponse::error("Path is not a directory")))
        }
        Err(FilesystemError::Io(e)) => {
            tracing::error!("Failed to read directory: {}", e);
            Ok(ResponseJson(ApiResponse::error(&format!(
                "Failed to read directory: {}",
                e
            ))))
        }
    }
}

fn build_file_tree(path: &std::path::Path, base_path: &std::path::Path, depth: u32, max_depth: u32) -> Option<FileTreeNode> {
    let name = path.file_name()?.to_string_lossy().to_string();

    // Skip hidden files and common non-essential directories
    if name.starts_with('.') || name == "node_modules" || name == "target" || name == "__pycache__" {
        return None;
    }

    let relative_path = path.strip_prefix(base_path).ok()?.to_string_lossy().to_string();

    if path.is_dir() {
        let children = if depth < max_depth {
            let mut entries: Vec<FileTreeNode> = std::fs::read_dir(path)
                .ok()?
                .filter_map(|e| e.ok())
                .filter_map(|e| build_file_tree(&e.path(), base_path, depth + 1, max_depth))
                .collect();

            // Sort: directories first, then files, alphabetically
            entries.sort_by(|a, b| {
                match (&a.node_type[..], &b.node_type[..]) {
                    ("directory", "file") => std::cmp::Ordering::Less,
                    ("file", "directory") => std::cmp::Ordering::Greater,
                    _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
                }
            });

            Some(entries)
        } else {
            None
        };

        Some(FileTreeNode {
            name,
            path: relative_path,
            node_type: "directory".to_string(),
            children,
        })
    } else {
        Some(FileTreeNode {
            name,
            path: relative_path,
            node_type: "file".to_string(),
            children: None,
        })
    }
}

pub async fn get_file_tree(
    Query(query): Query<FileTreeQuery>,
) -> Result<ResponseJson<ApiResponse<FileTreeResponse>>, ApiError> {
    let path = std::path::Path::new(&query.path);
    let max_depth = query.depth.unwrap_or(3);

    if !path.exists() {
        return Ok(ResponseJson(ApiResponse::error("Path does not exist")));
    }

    if !path.is_dir() {
        return Ok(ResponseJson(ApiResponse::error("Path is not a directory")));
    }

    let entries: Vec<FileTreeNode> = std::fs::read_dir(path)
        .map_err(|e| ApiError::Internal(format!("Failed to read directory: {}", e)))?
        .filter_map(|e| e.ok())
        .filter_map(|e| build_file_tree(&e.path(), path, 1, max_depth))
        .collect();

    let mut sorted_entries = entries;
    sorted_entries.sort_by(|a, b| {
        match (&a.node_type[..], &b.node_type[..]) {
            ("directory", "file") => std::cmp::Ordering::Less,
            ("file", "directory") => std::cmp::Ordering::Greater,
            _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
        }
    });

    Ok(ResponseJson(ApiResponse::success(FileTreeResponse {
        entries: sorted_entries,
    })))
}

pub async fn read_file(
    Query(query): Query<ReadFileQuery>,
) -> Result<ResponseJson<ApiResponse<FileContentResponse>>, ApiError> {
    let path = std::path::Path::new(&query.path);

    if !path.exists() {
        return Ok(ResponseJson(ApiResponse::error("File does not exist")));
    }

    if !path.is_file() {
        return Ok(ResponseJson(ApiResponse::error("Path is not a file")));
    }

    // Check file size - limit to 1MB for text preview
    let metadata = std::fs::metadata(path)
        .map_err(|e| ApiError::Internal(format!("Failed to read file metadata: {}", e)))?;

    if metadata.len() > 1_000_000 {
        return Ok(ResponseJson(ApiResponse::error("File is too large to preview (max 1MB)")));
    }

    let content = std::fs::read_to_string(path)
        .map_err(|e| ApiError::Internal(format!("Failed to read file: {}", e)))?;

    Ok(ResponseJson(ApiResponse::success(FileContentResponse {
        content,
        path: query.path,
    })))
}

pub fn router() -> Router<DeploymentImpl> {
    Router::new()
        .route("/filesystem/directory", get(list_directory))
        .route("/filesystem/git-repos", get(list_git_repos))
        .route("/filesystem/tree", get(get_file_tree))
        .route("/filesystem/file", get(read_file))
}
