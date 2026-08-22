use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TenantId(pub String);

impl std::fmt::Display for TenantId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Error, Debug, PartialEq)]
pub enum QuotaError {
    #[error("Tenant {0} not found")]
    TenantNotFound(TenantId),
    #[error("Max VMs quota reached (active: {active}, limit: {limit})")]
    ExceededMaxVms { active: u32, limit: u32 },
    #[error("Max RAM quota reached (requested: {requested_mib}MB, available: {available_mib}MB)")]
    ExceededMaxRam {
        requested_mib: u64,
        available_mib: u64,
    },
    #[error("Max vCPU quota reached (requested: {requested_vcpu}, available: {available_vcpu})")]
    ExceededMaxVcpu {
        requested_vcpu: u32,
        available_vcpu: u32,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TenantNamespace {
    pub id: TenantId,
    pub name: String,
    pub quota_max_vms: u32,
    pub quota_max_ram_mib: u64,
    pub quota_max_vcpu: u32,
    pub active_vms: u32,
    pub allocated_ram_mib: u64,
    pub allocated_vcpu: u32,
}

impl TenantNamespace {
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        quota_max_vms: u32,
        quota_max_ram_mib: u64,
        quota_max_vcpu: u32,
    ) -> Self {
        Self {
            id: TenantId(id.into()),
            name: name.into(),
            quota_max_vms,
            quota_max_ram_mib,
            quota_max_vcpu,
            active_vms: 0,
            allocated_ram_mib: 0,
            allocated_vcpu: 0,
        }
    }
}

#[derive(Clone, Default)]
pub struct TenantManager {
    tenants: Arc<RwLock<HashMap<TenantId, TenantNamespace>>>,
}

impl TenantManager {
    pub fn new() -> Self {
        Self {
            tenants: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn register_tenant(&self, namespace: TenantNamespace) {
        let mut map = self.tenants.write().unwrap();
        map.insert(namespace.id.clone(), namespace);
    }

    pub fn get_tenant(&self, id: &TenantId) -> Option<TenantNamespace> {
        let map = self.tenants.read().unwrap();
        map.get(id).cloned()
    }

    pub fn allocate(
        &self,
        tenant_id: &TenantId,
        ram_mib: u64,
        vcpu: u32,
    ) -> Result<(), QuotaError> {
        let mut map = self.tenants.write().unwrap();
        let tenant = map
            .get_mut(tenant_id)
            .ok_or_else(|| QuotaError::TenantNotFound(tenant_id.clone()))?;

        if tenant.active_vms + 1 > tenant.quota_max_vms {
            return Err(QuotaError::ExceededMaxVms {
                active: tenant.active_vms,
                limit: tenant.quota_max_vms,
            });
        }

        if tenant.allocated_ram_mib + ram_mib > tenant.quota_max_ram_mib {
            let avail = tenant
                .quota_max_ram_mib
                .saturating_sub(tenant.allocated_ram_mib);
            return Err(QuotaError::ExceededMaxRam {
                requested_mib: ram_mib,
                available_mib: avail,
            });
        }

        if tenant.allocated_vcpu + vcpu > tenant.quota_max_vcpu {
            let avail = tenant.quota_max_vcpu.saturating_sub(tenant.allocated_vcpu);
            return Err(QuotaError::ExceededMaxVcpu {
                requested_vcpu: vcpu,
                available_vcpu: avail,
            });
        }

        tenant.active_vms += 1;
        tenant.allocated_ram_mib += ram_mib;
        tenant.allocated_vcpu += vcpu;
        Ok(())
    }

    pub fn release(&self, tenant_id: &TenantId, ram_mib: u64, vcpu: u32) {
        let mut map = self.tenants.write().unwrap();
        if let Some(tenant) = map.get_mut(tenant_id) {
            tenant.active_vms = tenant.active_vms.saturating_sub(1);
            tenant.allocated_ram_mib = tenant.allocated_ram_mib.saturating_sub(ram_mib);
            tenant.allocated_vcpu = tenant.allocated_vcpu.saturating_sub(vcpu);
        }
    }
}
