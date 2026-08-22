# ⚙️ `mos.toml` Configuration Specification & Guide

> **MOS (MicroVM Operating Service) 프로젝트 설정 가이드**  
> `mos.toml`은 MOS가 애플리케이션을 빌드하고 MicroVM으로 구동할 때 필요한 리소스, 네트워크, 스토리지, 스케일링 정책을 정의하는 선택적(Optional) 설정 파일입니다.

---

## 📌 핵심 원칙: Zero-Config & Progressive Override

1. **파일이 없어도 작동 (Zero-Config Default)**:
   * 프로젝트 루트에 `mos.toml`이 없더라도, `mos deploy`는 Nixpacks 빌드 엔진과 프레임워크 자동 감지(Node.js, Python, Rust, Go 등)를 통해 기본 리소스(1 vCPU, 128MB RAM, Egress 완전 개방)로 즉시 MicroVM을 배포합니다.
2. **필요한 항목만 점진적 재정의 (Progressive Override)**:
   * 메모리 증설, GPU VRAM 할당, 전용 커스텀 도메인, 외부 아웃바운드 화이트리스트 방화벽, SQLite Litestream 복제 등 **커스텀 설정이 필요한 항목만 선별하여 `mos.toml`에 작성**하면 됩니다.

---

## 📑 전체 스키마 요약 (Full Schema Overview)

```toml
# ==============================================================================
# 1. 애플리케이션 기본 메타데이터 [app]
# ==============================================================================
[app]
name = "my-service"                  # 애플리케이션 식별자 (기본값: 디렉터리 이름)
version = "1.0.0"                    # 버전 태그
provider = "node"                    # 런타임 강제 지정 ("node" | "python" | "rust" | "go" | "static")
build_command = "npm run build"      # 커스텀 빌드 커맨드 (미지정 시 자동 감지)
start_command = "npm run start"      # 커스텀 실행 커맨드 (미지정 시 자동 감지)

# ==============================================================================
# 2. 하드웨어 가상화 리소스 쿼터 [resources]
# ==============================================================================
[resources]
vcpu = 1                             # 할당할 가상 CPU 코어 수 (기본값: 1)
memory_mib = 256                     # 할당할 메모리 크기 (MiB 단위, 기본값: 128)
# gpu_vram_mib = 8192                # Scale-to-Zero GPU VRAM 할당 (AI/LLM 모델 구동 시)

# ==============================================================================
# 3. 네트워크 및 인그레스 / 이그레스 방화벽 [network]
# ==============================================================================
[network]
port = 3000                          # 게스트 애플리케이션이 수신 대기하는 포트 (기본값: 프레임워크 기본 포트)
domain = "my-service.mos.local"      # 호스팅 도메인 (기본값: <app-name>.mos.local)
tls = "auto"                         # TLS 인증서 모드 ("auto" | "self-signed" | "off")

# 외부 아웃바운드(Egress) 방화벽 모드
#  - "allow-all": 모든 외부 네트워크 통신 완전 개방 (기본값 - GA, Sentry, 외부 API 자유롭게 호출)
#  - "whitelist-only": allowed_outbound 에 등록된 도메인/IP만 커널 레벨 허용
egress = "allow-all"

# 화이트리스트 모드일 때 통신을 허용할 외부 FQDN / 엔드포인트 목록
allowed_outbound = [
    "o12345.ingest.sentry.io",
    "www.google-analytics.com",
    "generativelanguage.googleapis.com",
    "api.openai.com",
    "api.anthropic.com",
    "api.rybbit.com"
]

# ==============================================================================
# 4. 스토리지 및 SQLite Litestream 실시간 복제 [storage]
# ==============================================================================
[storage]
# 분산 공유 볼륨 마운트 (선택 사항)
# volumes = [
#     { name = "shared-uploads", mount_path = "/app/uploads", mode = "rwx" }
# ]

# SQLite 실시간 S3 / Cloudflare R2 스트리밍 복제 설정
[storage.litestream]
enabled = true                       # SQLite 파일 발견 시 자동 활성화 (기본값: auto)
db_path = "data/app.db"              # SQLite 데이터베이스 파일 경로
replica_type = "s3"                  # 복제 대상 스토리지 ("s3" | "gcs" | "abs")
bucket = "my-app-db-replicas"        # S3 / R2 버킷 이름
# s3_endpoint = "https://<account-id>.r2.cloudflarestorage.com" # Cloudflare R2 등 호환 엔드포인트

# ==============================================================================
# 5. Scale-to-Zero 및 수명주기 스케일링 [scaling]
# ==============================================================================
[scaling]
idle_timeout_seconds = 300           # 무트래픽 지속 시 스냅샷 절전 진입 시간 (초 단위, 기본값: 300)
strategy = "uffd"                    # 복구 가속 전략 ("uffd" [1.2ms 지연 페이징] | "snapshot" [6.5ms 풀 메모리])
min_instances = 0                    # 최소 인스턴스 (0 = True Scale-to-Zero)
max_instances = 10                   # 최대 오토스케일링 확장 인스턴스 수

# ==============================================================================
# 6. 점진적 카나리 배포 파이프라인 [canary]
# ==============================================================================
[canary]
enabled = false                      # 점진적 카나리 승격 활성화 여부
initial_weight = 10                  # 초기 카나리 트래픽 비율 (10%)
step_weights = [10, 50, 100]         # 승격 단계 (10% -> 50% -> 100%)
step_interval_seconds = 60           # 다음 단계 승격 대기 시간
error_threshold_pct = 1.0            # 5xx 에러율 임계치 (1.0% 초과 시 즉각 자동 롤백)

# ==============================================================================
# 7. 환경 변수 [env]
# ==============================================================================
[env]
NODE_ENV = "production"
PORT = "3000"
LOG_LEVEL = "info"
```

---

## 🎯 실전 워크로드별 설정 예시 (Recipes)

### 예시 1: Next.js 14 SSR 풀스택 웹 애플리케이션
Sentry 에러 추적과 Google Analytics가 포함된 프로덕션 Next.js 앱:

```toml
[app]
name = "nextjs-storefront"

[resources]
vcpu = 2
memory_mib = 512

[network]
port = 3000
domain = "store.example.com"
tls = "auto"
egress = "allow-all" # GA, Sentry 외부 시그널 송출

[env]
NODE_ENV = "production"
NEXT_TELEMETRY_DISABLED = "1"
```

---

### 예시 2: FastAPI + SQLite + Litestream S3 백업 백엔드
외부 DB 서버 없이 로컬 SQLite로 동작하며, 유휴 시 0MB로 절전되고 트랜잭션이 S3로 실시간 스트리밍되는 구성:

```toml
[app]
name = "fastapi-backend"

[resources]
vcpu = 1
memory_mib = 256

[network]
port = 8000
domain = "api.example.com"

[storage.litestream]
enabled = true
db_path = "app.db"
replica_type = "s3"
bucket = "my-fastapi-db-backups"

[scaling]
idle_timeout_seconds = 180
strategy = "uffd" # 1.2ms 초고속 기상
```

---

### 예시 3: Scale-to-Zero GPU LLM 인퍼런스 서버
평소에는 GPU VRAM을 0MB로 완전히 반납하고, 사용자의 AI 추론 요청이 들어올 때 24GB VRAM을 즉각 바인딩하는 LLM 서빙 구성:

```toml
[app]
name = "llama3-inference-service"

[resources]
vcpu = 4
memory_mib = 4096
gpu_vram_mib = 16384 # 16GB GPU VRAM 동적 풀링

[network]
port = 8000
domain = "ai.example.com"

[scaling]
idle_timeout_seconds = 60 # 유휴 60초 후 GPU VRAM 즉각 회수 (0MB 절전)
strategy = "uffd"
```

---

### 예시 4: 금융 / 엄격한 보안 격리 백엔드 (Strict Whitelist Egress)
외부 데이터 유출을 막기 위해 지정된 엔드포인트 외 모든 외부 아웃바운드 패킷을 커널 레벨에서 차단:

```toml
[app]
name = "fintech-settlement-engine"

[resources]
vcpu = 2
memory_mib = 1024

[network]
port = 8080
domain = "settle.internal.local"
egress = "whitelist-only" # 화이트리스트 외 전면 차단
allowed_outbound = [
    "api.iamport.kr",
    "pg-api.tosspayments.com",
    "o99999.ingest.sentry.io"
]
```

---

## 🔍 설정 유효성 검사

배포 전 로컬에서 설정 파일 문법 및 스펙 유효성을 검증할 수 있습니다:

```bash
# 현재 디렉터리의 mos.toml 검증 및 Nixpacks 빌드 플랜 미리보기
mos deploy --dry-run
```
