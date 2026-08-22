# 🌐 MOS Ingress & Edge Routing Guide

> **MOS (MicroVM Operating Service) 인그레스 프록시 및 도메인 라우팅 가이드**  
> `mos-edge`는 단일 물리 베어메탈 서버에서 수천 개의 독립된 MicroVM 및 외부 서비스를 단일 진입점(포트 80/443/8180 등)으로 중계하는 초고성능 리버스 프록시입니다.

---

## 📌 2가지 라우팅 방식 (Routing Architecture)

MOS는 **개발자 워크플로우**와 **인프라 운영자 워크플로우**를 분리하여 2가지 라우팅 방식을 모두 지원합니다.

```
                                [ Ingress Traffic (*.example.com) ]
                                                │
                                                ▼
                        ┌───────────────────────────────────────────────┐
                        │ 🌐 mos-edge (High-Performance Ingress Proxy)   │
                        │    - Sub-millisecond Hash Table Routing       │
                        │    - TCP/HTTP Request Buffering               │
                        │    - Wake-on-HTTP IPC Trigger                 │
                        └───────────────┬───────────────┬───────────────┘
                                        │               │
            ┌───────────────────────────┘               └───────────────────────────┐
            ▼                                                                       ▼
┌───────────────────────────────────────┐                       ┌───────────────────────────────────────┐
│ 1. 동적 MicroVM 라우팅 (Dynamic)       │                       │ 2. 정적 게이트웨이 라우팅 (Static)    │
│  - `mos deploy ./app` 실행 시          │                       │  - `config/routes.json` 파일 로드     │
│  - 배포 파이프라인이 메모리에 자동 등록 │                       │  - 다중 서비스/포트 일괄 매핑 관리    │
│  - Scale-to-Zero 기상 시 IP 동적 갱신 │                       │  - Nginx/Caddy 대체 인그레스 게이트웨이│
└───────────────────────────────────────┘                       └───────────────────────────────────────┘
```

---

## 1. 정적 게이트웨이 라우팅 (`config/routes.json`)

Nginx의 `nginx.conf`나 Caddy의 `Caddyfile`처럼, 여러 개의 서브도메인과 내부 포트를 선언적으로 관리할 때 사용합니다.

### 템플릿 복사 및 설정
```bash
# 1. 템플릿 파일을 복사하여 나만의 routes.json 생성 (Git에 추적되지 않음)
cp config/routes.example.json config/routes.json

# 2. 원하는 도메인 및 포트 매핑 작성
```

### `config/routes.json` 스키마 규격
```json
{
  "routes": {
    "app.example.com": {
      "host": "127.0.0.1",
      "port": 8080,
      "is_suspended": false
    },
    "api.example.com": {
      "host": "127.0.0.1",
      "port": 8000,
      "is_suspended": false
    },
    "worker.example.com": {
      "host": "127.0.0.1",
      "port": 8085,
      "is_suspended": true
    }
  }
}
```

* **`host`**: 포워딩할 대상 호스트 (기본 `127.0.0.1` 또는 MicroVM 내부 IP `172.16.x.x`)
* **`port`**: 대상 서비스가 수신 대기 중인 TCP 포트
* **`is_suspended`**:
  * `false`: 일반 상시 구동 서비스 (즉시 프록시 포워딩)
  * `true`: **Scale-to-Zero 절전 상태**의 MicroVM (첫 요청 시 HTTP 패킷을 메모리에 버퍼링하고, Orchestrator에 기상 신호를 보낸 후 1.20ms 내 즉각 포워딩)

### Edge 프록시 실행 커맨드
```bash
# 기본 config/routes.json 자동 로드
mos edge --port 8180

# 또는 커스텀 설정 파일 경로 지정
mos edge --port 8180 --config /etc/mos/production-routes.json
```

---

## 2. 동적 MicroVM 자동 라우팅 (`mos deploy`)

개발자가 소스코드를 배포할 때는 설정 파일을 직접 편집할 필요가 없습니다.

1. **배포 시 자동 등록**:
   ```bash
   mos deploy ./my-nextjs-app
   ```
   * 빌드 완료 즉시 `mos-builder`가 `my-nextjs-app.mos.local -> 172.16.0.2:3000` 라우팅 엔트리를 `mos-edge` 인메모리 라우팅 테이블에 실시간 등록합니다.
2. **Scale-to-Zero 절전 전환**:
   * 300초간 트래픽이 없으면 `mos-orchestrator`가 메모리 스냅샷을 뜨고, 라우팅 테이블의 `is_suspended` 플래그를 `true`로 자동 전환합니다.
3. **Wake-on-HTTP 자동 복구**:
   * 브라우저에서 `my-nextjs-app.mos.local` 접속 시 `mos-edge`가 요청을 버퍼링한 뒤 VM을 깨우고 `is_suspended = false`로 원복하여 0% 패킷 유실로 응답을 전달합니다.

---

## 🛡️ 고급 기능 (Advanced Features)

### 1. 3단계 가중치 카나리 배포 (Weighted Canary)
새로운 버전을 배포할 때 점진적으로 트래픽을 승격합니다:
```bash
# 20% 트래픽을 카나리 인스턴스로 분기
# GitHub Push Webhook 연동 시 10% -> 50% -> 100% 자동 승격 및 에러 시 즉각 롤백
```

### 2. 자동 TLS & ACME 인증서
* `mos.dev`, `*.example.com` 와일드카드 및 개별 도메인에 대해 Let's Encrypt HTTP-01 챌린지 자동 응답 및 무중단 인증서 갱신을 지원합니다.

### 3. eBPF / XDP DDoS 커널 방어
* 초당 임계치를 초과하는 비정상 패킷 플러딩을 Linux 커널 L4 단계에서 즉각 드롭(`XDP_DROP`)하여 프록시 워커 스레드의 고갈을 방지합니다.

### 4. W3C 분산 트레이싱 (`traceparent`)
* 모든 인그레스 요청에 대해 `ingress`, `routing`, `wake`, `guest_exec` 단계별 마이크로초(µs) 정밀 레이턴시를 측정하고 분산 트레이싱 헤더를 전파합니다.
