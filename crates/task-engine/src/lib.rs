use std::{collections::HashMap, sync::Arc};

use chrono::Utc;
use ozon_domain::{
    ApprovalId, ApprovalRecord, AuditEvent, AuditEventId, DomainError, DryRunDiff,
    ExecutionReceipt, OperationKind, RiskLevel, Task, TaskId, TaskSource, TaskState, TenantId,
};
use thiserror::Error;
use tokio::sync::{RwLock, broadcast};

#[derive(Clone, Debug)]
pub struct CreateTask {
    pub tenant_id: TenantId,
    pub shop_id: String,
    pub source: TaskSource,
    pub operation: OperationKind,
    pub dry_run: DryRunDiff,
    pub risk: RiskLevel,
    pub idempotency_key: String,
}

#[derive(Clone, Debug)]
pub struct TaskStore {
    inner: Arc<RwLock<HashMap<TaskId, Task>>>,
    audit: Arc<RwLock<Vec<AuditEvent>>>,
    events: broadcast::Sender<TaskEvent>,
}

impl Default for TaskStore {
    fn default() -> Self {
        Self::new()
    }
}

impl TaskStore {
    pub fn new() -> Self {
        let (events, _) = broadcast::channel(256);
        Self {
            inner: Arc::new(RwLock::new(HashMap::new())),
            audit: Arc::new(RwLock::new(Vec::new())),
            events,
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<TaskEvent> {
        self.events.subscribe()
    }

    pub async fn create_dry_run(&self, input: CreateTask) -> Result<Task, TaskEngineError> {
        let now = Utc::now();
        let state = if input.operation.is_write() {
            TaskState::PendingApproval
        } else {
            TaskState::Queued
        };
        let task = Task {
            id: TaskId::new(),
            tenant_id: input.tenant_id,
            shop_id: input.shop_id,
            source: input.source,
            operation: input.operation,
            state,
            dry_run: input.dry_run,
            risk: input.risk,
            idempotency_key: input.idempotency_key,
            approval: None,
            receipt: None,
            created_at: now,
            updated_at: now,
        };
        self.inner.write().await.insert(task.id, task.clone());
        self.audit(
            Some(task.tenant_id),
            "system",
            "task.created",
            &format!("{:?}", task.id),
            &format!("{:?} dry-run created", task.operation),
        )
        .await;
        let _ = self.events.send(TaskEvent::Changed(task.clone()));
        Ok(task)
    }

    pub async fn get(&self, id: TaskId) -> Option<Task> {
        self.inner.read().await.get(&id).cloned()
    }

    pub async fn list(&self) -> Vec<Task> {
        let mut tasks: Vec<_> = self.inner.read().await.values().cloned().collect();
        tasks.sort_by_key(|task| task.created_at);
        tasks
    }

    pub async fn approve(
        &self,
        id: TaskId,
        approved_by: String,
        note: Option<String>,
    ) -> Result<Task, TaskEngineError> {
        let mut inner = self.inner.write().await;
        let task = inner.get_mut(&id).ok_or(TaskEngineError::TaskNotFound)?;
        if !task.operation.is_write() {
            return Err(TaskEngineError::ApprovalNotRequired);
        }
        transition(task, TaskState::Queued)?;
        task.approval = Some(ApprovalRecord {
            id: ApprovalId::new(),
            approved_by,
            approved_at: Utc::now(),
            note,
        });
        let task = task.clone();
        drop(inner);
        self.audit(
            Some(task.tenant_id),
            "approver",
            "task.approved",
            &format!("{:?}", task.id),
            "write task approved and queued",
        )
        .await;
        let _ = self.events.send(TaskEvent::Changed(task.clone()));
        Ok(task)
    }

    pub async fn cancel(&self, id: TaskId, actor: &str) -> Result<Task, TaskEngineError> {
        let mut inner = self.inner.write().await;
        let task = inner.get_mut(&id).ok_or(TaskEngineError::TaskNotFound)?;
        transition(task, TaskState::Cancelled)?;
        let task = task.clone();
        drop(inner);
        self.audit(
            Some(task.tenant_id),
            actor,
            "task.cancelled",
            &format!("{:?}", task.id),
            "task cancelled",
        )
        .await;
        let _ = self.events.send(TaskEvent::Changed(task.clone()));
        Ok(task)
    }

    pub async fn mark_running(&self, id: TaskId) -> Result<Task, TaskEngineError> {
        let mut inner = self.inner.write().await;
        let task = inner.get_mut(&id).ok_or(TaskEngineError::TaskNotFound)?;
        if task.operation.is_write() && task.approval.is_none() {
            return Err(TaskEngineError::Domain(DomainError::WriteRequiresApproval));
        }
        transition(task, TaskState::Running)?;
        let task = task.clone();
        let _ = self.events.send(TaskEvent::Changed(task.clone()));
        Ok(task)
    }

    pub async fn mark_succeeded(
        &self,
        id: TaskId,
        receipt: ExecutionReceipt,
    ) -> Result<Task, TaskEngineError> {
        let mut inner = self.inner.write().await;
        let task = inner.get_mut(&id).ok_or(TaskEngineError::TaskNotFound)?;
        transition(task, TaskState::Succeeded)?;
        task.receipt = Some(receipt);
        let task = task.clone();
        let _ = self.events.send(TaskEvent::Changed(task.clone()));
        Ok(task)
    }

    pub async fn audit_log(&self) -> Vec<AuditEvent> {
        self.audit.read().await.clone()
    }

    async fn audit(
        &self,
        tenant_id: Option<TenantId>,
        actor: &str,
        action: &str,
        target: &str,
        summary: &str,
    ) {
        self.audit.write().await.push(AuditEvent {
            id: AuditEventId::new(),
            tenant_id,
            actor: actor.to_string(),
            action: action.to_string(),
            target: target.to_string(),
            summary: summary.to_string(),
            created_at: Utc::now(),
        });
    }
}

fn transition(task: &mut Task, next: TaskState) -> Result<(), TaskEngineError> {
    if !task.state.can_transition_to(next) {
        return Err(TaskEngineError::Domain(
            DomainError::InvalidTaskTransition {
                from: task.state,
                to: next,
            },
        ));
    }
    task.state = next;
    task.updated_at = Utc::now();
    Ok(())
}

#[derive(Clone, Debug)]
pub enum TaskEvent {
    Changed(Task),
}

#[derive(Debug, Error)]
pub enum TaskEngineError {
    #[error("task not found")]
    TaskNotFound,
    #[error("approval is not required for this operation")]
    ApprovalNotRequired,
    #[error(transparent)]
    Domain(#[from] DomainError),
}

#[cfg(test)]
mod tests {
    use ozon_domain::{DryRunDiff, RiskLevel};

    use super::*;

    fn dry_run() -> DryRunDiff {
        DryRunDiff {
            summary: "mock price update".to_string(),
            target_count: 1,
            changes: vec![],
            warnings: vec![],
        }
    }

    #[tokio::test]
    async fn write_task_requires_approval_before_running() {
        let store = TaskStore::new();
        let task = store
            .create_dry_run(CreateTask {
                tenant_id: TenantId::new(),
                shop_id: "shop-1".to_string(),
                source: TaskSource::OpenClaw,
                operation: OperationKind::OzonUpdatePriceMock,
                dry_run: dry_run(),
                risk: RiskLevel::High,
                idempotency_key: "idem-1".to_string(),
            })
            .await
            .unwrap();

        assert_eq!(task.state, TaskState::PendingApproval);
        assert!(store.mark_running(task.id).await.is_err());

        let approved = store
            .approve(task.id, "operator".to_string(), None)
            .await
            .unwrap();
        assert_eq!(approved.state, TaskState::Queued);
        assert!(store.mark_running(task.id).await.is_ok());
    }

    #[tokio::test]
    async fn cancelled_write_task_is_terminal() {
        let store = TaskStore::new();
        let task = store
            .create_dry_run(CreateTask {
                tenant_id: TenantId::new(),
                shop_id: "shop-1".to_string(),
                source: TaskSource::OpenClaw,
                operation: OperationKind::OzonUpdateInventoryMock,
                dry_run: dry_run(),
                risk: RiskLevel::High,
                idempotency_key: "idem-cancel".to_string(),
            })
            .await
            .unwrap();

        let cancelled = store.cancel(task.id, "operator").await.unwrap();
        assert_eq!(cancelled.state, TaskState::Cancelled);
        assert!(
            store
                .approve(task.id, "operator".to_string(), None)
                .await
                .is_err()
        );
        assert!(store.mark_running(task.id).await.is_err());
    }

    #[tokio::test]
    async fn succeeded_task_is_terminal() {
        let store = TaskStore::new();
        let task = store
            .create_dry_run(CreateTask {
                tenant_id: TenantId::new(),
                shop_id: "shop-1".to_string(),
                source: TaskSource::LocalUi,
                operation: OperationKind::OzonProductsList,
                dry_run: dry_run(),
                risk: RiskLevel::Low,
                idempotency_key: "idem-read".to_string(),
            })
            .await
            .unwrap();

        store.mark_running(task.id).await.unwrap();
        let succeeded = store
            .mark_succeeded(
                task.id,
                ExecutionReceipt {
                    external_request_id: None,
                    executed_at: Utc::now(),
                    result_summary: "read completed".to_string(),
                },
            )
            .await
            .unwrap();

        assert_eq!(succeeded.state, TaskState::Succeeded);
        assert!(store.cancel(task.id, "operator").await.is_err());
        assert!(store.mark_running(task.id).await.is_err());
    }
}
