use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UsageMetric {
    pub vcpu_seconds: f64,
    pub ram_gib_seconds: f64,
    pub vram_gib_seconds: f64,
    pub egress_bytes: u64,
}

impl UsageMetric {
    pub fn add(&mut self, other: &UsageMetric) {
        self.vcpu_seconds += other.vcpu_seconds;
        self.ram_gib_seconds += other.ram_gib_seconds;
        self.vram_gib_seconds += other.vram_gib_seconds;
        self.egress_bytes += other.egress_bytes;
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BillingRate {
    /// USD per vCPU-second
    pub vcpu_sec_rate: f64,
    /// USD per GiB-second RAM
    pub ram_gib_sec_rate: f64,
    /// USD per GiB-second GPU VRAM
    pub vram_gib_sec_rate: f64,
    /// USD per GiB network egress
    pub egress_gib_rate: f64,
}

impl Default for BillingRate {
    fn default() -> Self {
        Self {
            vcpu_sec_rate: 0.000010,     // ~$0.036 / vCPU-hour
            ram_gib_sec_rate: 0.000002,  // ~$0.0072 / GiB-hour
            vram_gib_sec_rate: 0.000050, // ~$0.18 / GiB-hour VRAM
            egress_gib_rate: 0.05,       // $0.05 / GiB egress
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreditAccount {
    pub tenant_id: String,
    pub balance_credits: f64,
    pub auto_suspend_threshold: f64,
    pub is_suspended: bool,
    pub total_charged: f64,
    pub last_updated: DateTime<Utc>,
}

impl CreditAccount {
    pub fn new(tenant_id: impl Into<String>, initial_credits: f64) -> Self {
        Self {
            tenant_id: tenant_id.into(),
            balance_credits: initial_credits,
            auto_suspend_threshold: 0.0,
            is_suspended: false,
            total_charged: 0.0,
            last_updated: Utc::now(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BillingTransaction {
    pub id: String,
    pub tenant_id: String,
    pub amount: f64,
    pub description: String,
    pub timestamp: DateTime<Utc>,
}

#[derive(Error, Debug)]
pub enum BillingError {
    #[error(
        "Account suspended due to insufficient balance: current {balance}, threshold {threshold}"
    )]
    AccountSuspended { balance: f64, threshold: f64 },

    #[error("Tenant account not found: {0}")]
    AccountNotFound(String),

    #[error("Invalid transaction amount: {0}")]
    InvalidAmount(f64),
}

#[derive(Clone)]
pub struct BillingEngine {
    accounts: Arc<DashMap<String, CreditAccount>>,
    rates: BillingRate,
    transactions: Arc<DashMap<String, Vec<BillingTransaction>>>,
}

impl BillingEngine {
    pub fn new(rates: BillingRate) -> Self {
        Self {
            accounts: Arc::new(DashMap::new()),
            rates,
            transactions: Arc::new(DashMap::new()),
        }
    }

    pub fn register_account(&self, account: CreditAccount) {
        self.accounts.insert(account.tenant_id.clone(), account);
    }

    pub fn get_account(&self, tenant_id: &str) -> Option<CreditAccount> {
        self.accounts.get(tenant_id).map(|r| r.clone())
    }

    pub fn topup_credit(&self, tenant_id: &str, amount: f64) -> Result<f64, BillingError> {
        if amount <= 0.0 {
            return Err(BillingError::InvalidAmount(amount));
        }

        let mut account = self
            .accounts
            .get_mut(tenant_id)
            .ok_or_else(|| BillingError::AccountNotFound(tenant_id.to_string()))?;

        account.balance_credits += amount;
        if account.balance_credits > account.auto_suspend_threshold {
            account.is_suspended = false;
        }
        account.last_updated = Utc::now();

        let tx = BillingTransaction {
            id: Uuid::new_v4().to_string(),
            tenant_id: tenant_id.to_string(),
            amount,
            description: "Credit top-up".to_string(),
            timestamp: Utc::now(),
        };
        self.transactions
            .entry(tenant_id.to_string())
            .or_default()
            .push(tx);

        Ok(account.balance_credits)
    }

    pub fn calculate_cost(&self, metric: &UsageMetric) -> f64 {
        let vcpu_cost = metric.vcpu_seconds * self.rates.vcpu_sec_rate;
        let ram_cost = metric.ram_gib_seconds * self.rates.ram_gib_sec_rate;
        let vram_cost = metric.vram_gib_seconds * self.rates.vram_gib_sec_rate;
        let egress_gib = (metric.egress_bytes as f64) / (1024.0 * 1024.0 * 1024.0);
        let egress_cost = egress_gib * self.rates.egress_gib_rate;

        vcpu_cost + ram_cost + vram_cost + egress_cost
    }

    pub fn charge_usage(&self, tenant_id: &str, metric: &UsageMetric) -> Result<f64, BillingError> {
        let cost = self.calculate_cost(metric);
        let mut account = self
            .accounts
            .get_mut(tenant_id)
            .ok_or_else(|| BillingError::AccountNotFound(tenant_id.to_string()))?;

        account.balance_credits -= cost;
        account.total_charged += cost;
        account.last_updated = Utc::now();

        if account.balance_credits <= account.auto_suspend_threshold {
            account.is_suspended = true;
        }

        let tx = BillingTransaction {
            id: Uuid::new_v4().to_string(),
            tenant_id: tenant_id.to_string(),
            amount: -cost,
            description: format!("Usage charge: {:.6} credits", cost),
            timestamp: Utc::now(),
        };
        self.transactions
            .entry(tenant_id.to_string())
            .or_default()
            .push(tx);

        if account.is_suspended {
            return Err(BillingError::AccountSuspended {
                balance: account.balance_credits,
                threshold: account.auto_suspend_threshold,
            });
        }

        Ok(account.balance_credits)
    }
}
