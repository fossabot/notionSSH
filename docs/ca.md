# CA 인증서 검증 (Certificate Verification)

NotionSSH는 중간자 공격(MITM)을 방지하기 위해 다층 보안 검증을 제공합니다. 이 기능은 Notion API 서버와의 통신 시 인증서 체인 검증, DoH DNS 검증, 인증서 핀(Certificate Pinning)을 통해 보안을 강화합니다.

## 보안 검증 단계

NotionSSH는 3단계 검증을 수행합니다:

1. **CA 인증서 체인 검증 (1/3)**: 표준 CA를 통한 인증서 체인 검증
2. **DoH DNS 검증 (2/3)**: DNS over HTTPS를 통한 IP 주소 검증
3. **인증서 핀 검증 (3/3)**: 사전 정의된 인증서 지문과 비교

## 설정 방법

### 자동 설정 (권장)

NotionSSH를 처음 실행할 때 CA 인증서 검증을 사용할지 묻는 메시지가 표시됩니다:

```
Use CA public key fingerprint verification? [Y/N]
```

- **Y 입력**: CA 인증서 검증 활성화
- **N 입력**: 기본 TLS 검증만 사용

### 수동 설정

#### 1. verify 디렉터리 구성

프로젝트 루트의 `verify/` 디렉터리에 인증서 지문을 저장합니다.

**파일 구조:**
```
verify/
├── notion-api.verify        # JSON 형식 (권장)
├── README.txt              # 설정 방법 안내
└── notion-api.verify.example # 예시 파일
```

#### 2. notion-api.verify 파일 형식

```json
{
  "leaf_sha256": [
    "LEAF_CERTIFICATE_SHA256_FINGERPRINT_HERE"
  ],
  "ca_der_sha256": [
    "CA_CERTIFICATE_DER_SHA256_HERE"
  ],
  "ca_spki_sha256": [
    "CA_PUBLIC_KEY_SPKI_SHA256_HERE"
  ]
}
```

#### 3. .notionSSH/ca.json 파일

런타임에 자동 생성되는 CA 핀 저장소:

```json
{
  "note": "Stored CA public key for api.notion.com",
  "host": "api.notion.com",
  "spki_sha256": "CA_PUBLIC_KEY_SPKI_SHA256",
  "der_sha256": "CA_CERTIFICATE_DER_SHA256",
  "stored_at": "2024-01-15T10:30:45Z"
}
```

## 인증서 지문 획득 방법

### 자동 획득 (스크립트 사용)

프로젝트에 포함된 Python 스크립트를 사용합니다:

```bash
python scripts/update_notion_verify.py --yes
```

**스크립트 옵션:**
- `--host`: 대상 호스트 (기본값: api.notion.com)
- `--port`: 포트 번호 (기본값: 443)
- `--output`: 출력 파일 (기본값: verify/notion-api.verify)
- `--yes`: 확인 없이 자동 실행

### 수동 획득 (OpenSSL 사용)

#### 리프 인증서 지문 획득

```bash
echo | openssl s_client -connect api.notion.com:443 -servername api.notion.com 2>/dev/null \
  | openssl x509 -outform DER \
  | openssl dgst -sha256 -binary \
  | xxd -p -c 64 | tr '[:lower:]' '[:upper:]'
```

#### CA 인증서 지문 획득

```bash
echo | openssl s_client -connect api.notion.com:443 -servername api.notion.com -showcerts \
  | awk '/BEGIN CERTIFICATE/{i++} i==2, /END CERTIFICATE/' \
  | openssl x509 -outform DER \
  | openssl dgst -sha256 -binary \
  | xxd -p -c 64 | tr '[:lower:]' '[:upper:]'
```

## 검증 과정 상세

### 1단계: CA 인증서 체인 검증

- **목적**: 표준 CA를 통한 기본 TLS 검증
- **방법**: rustls와 webpki_roots를 사용한 체인 검증
- **성공**: `[*] (1/3) CA certificate chain: PASS`
- **실패**: 연결 즉시 종료

### 2단계: DoH DNS 검증

- **목적**: DNS 스푸핑 공격 방지
- **방법**: 
  - Cloudflare DoH (1.1.1.1)와 Google DoH (8.8.8.8)로 DNS 조회
  - 시스템 DNS 결과와 교차 검증
- **성공**: `[*] (2/3) DoH DNS verification: PASS`
- **실패**: `[!] (2/3) DoH DNS verification: FAIL`

### 3단계: 인증서 핀 검증

- **목적**: 특정 인증서/CA 강제 검증
- **방법**:
  - 리프 인증서 SHA256 지문 비교
  - CA 인증서 DER/SPKI SHA256 지문 비교
- **성공**: `[*] (3/3) Certificate pinning: PASS`
- **실패**: 
  - `[!] (3/3) Certificate pinning: FAIL (leaf mismatch)`
  - `[!] (3/3) Certificate pinning: FAIL (CA pin mismatch)`

## 자동 업데이트 기능

### CA 핀 자동 업데이트

인증서 핀 검증이 실패하면 새로운 CA 인증서로 업데이트할지 묻습니다:

```
[?] Do you want to update CA pin with the newly observed certificate? [Y/N]
```

- **Y 입력**: `.notionSSH/ca.json`에 새 CA 핀 저장
- **N 입력**: 프로그램 종료

### 리프 인증서 업데이트

리프 인증서 불일치 시 Python 스크립트 사용을 권장합니다:

```
[!] To refresh pins, run: python scripts/update_notion_verify.py --yes
```

## 보안 권장사항

### 정기적인 핀 업데이트
- Notion이 인증서를 갱신할 때마다 핀 업데이트 필요
- 자동화 스크립트를 주기적으로 실행하여 핀 동기화
- 백업용 핀을 미리 준비하여 서비스 중단 방지

### 검증 모드 선택
```bash
# 최대 보안 (권장 운영 환경)
# Y를 선택하여 3단계 모든 검증 활성화

# 기본 보안 (개발/테스트 환경)
# N을 선택하여 CA 체인 검증만 사용
```

### 네트워크 환경 고려
- **방화벽 환경**: DoH DNS 조회가 차단될 수 있음
- **프록시 환경**: 인증서 핀 검증이 실패할 수 있음
- **제한된 환경**: 기본 TLS 검증만 사용 고려

## 설정 예시

### 기본 설정 (신규 설치)

1. NotionSSH 첫 실행
2. CA 검증 활성화 선택 (Y)
3. 자동으로 `.notionSSH/ca.json` 생성
4. 정상 동작 확인

### 수동 설정 (고급 사용자)

1. `verify/notion-api.verify` 파일 준비:
```json
{
  "leaf_sha256": ["실제_리프_인증서_지문"],
  "ca_der_sha256": ["실제_CA_DER_지문"],
  "ca_spki_sha256": ["실제_CA_SPKI_지문"]
}
```

2. NotionSSH 실행 시 자동으로 `.notionSSH/ca.json` 생성
3. 3단계 모든 검증 통과 확인

### 업데이트 시나리오

**인증서 갱신으로 인한 실패:**
```bash
# 1. 스크립트로 새 핀 획득
python scripts/update_notion_verify.py --yes

# 2. NotionSSH 재시작
./notionSSH

# 3. 검증 통과 확인
[*] (1/3) CA certificate chain: PASS
[*] (2/3) DoH DNS verification: PASS
[*] (3/3) Certificate pinning: PASS
```

## 문제 해결

### 일반적인 오류

#### "해당 환경에서 CA인증서 인증에 실패했습니다. (1/3)"
- **원인**: 기본 TLS 연결 실패
- **해결**: 네트워크 연결 및 방화벽 설정 확인

#### "(2/3) DoH DNS verification: FAIL"
- **원인**: DoH DNS 조회 실패 또는 IP 불일치
- **해결**: 
  - 인터넷 연결 확인
  - DoH 서비스 차단 여부 확인
  - 프록시 설정 확인

#### "(3/3) Certificate pinning: FAIL (leaf mismatch)"
- **원인**: 리프 인증서가 변경됨
- **해결**: `python scripts/update_notion_verify.py --yes` 실행

#### "(3/3) Certificate pinning: FAIL (CA pin mismatch)"
- **원인**: CA 인증서가 변경됨
- **해결**: Y를 선택하여 CA 핀 자동 업데이트

### 설정 파일 오류

#### notion-api.verify 파일 손상
```bash
# 백업에서 복원하거나 재생성
python scripts/update_notion_verify.py --yes
```

#### .notionSSH/ca.json 파일 손상
```bash
# 파일 삭제 후 재실행
rm .notionSSH/ca.json
./notionSSH
```

### 환경별 설정

#### 개발 환경
- CA 검증 비활성화 (N 선택)
- 기본 TLS 검증만 사용
- 빠른 개발 사이클 지원

#### 스테이징 환경
- CA 검증 활성화 (Y 선택)
- 정기적인 핀 업데이트 스크립트 실행
- 운영 환경과 동일한 보안 설정

#### 운영 환경
- CA 검증 활성화 (Y 선택)
- 자동화된 핀 업데이트
- 모니터링 및 알림 설정
- 인증서 갱신 일정 추적

## 스크립트 도구

### update_notion_verify.py

**기본 사용법:**
```bash
python scripts/update_notion_verify.py
```

**고급 옵션:**
```bash
# 다른 호스트 지정
python scripts/update_notion_verify.py --host custom.api.com --port 443

# 출력 파일 지정
python scripts/update_notion_verify.py --output custom-verify.json

# 확인 없이 자동 실행
python scripts/update_notion_verify.py --yes
```

**의존성:**
- Python 3.6+
- `cryptography` 패키지 (AIA 확장을 통한 CA 인증서 획득)
- 선택사항: OpenSSL (커맨드라인 도구)

## 보안 고려사항

### 장점
- **MITM 공격 방지**: 인증서 핀을 통한 강력한 인증
- **DNS 스푸핑 방지**: 다중 DoH 서비스 교차 검증
- **자동 업데이트**: 사용자 개입 최소화
- **다층 보안**: 3단계 독립적 검증

### 주의사항
- **운영 복잡성**: 인증서 갱신 시 핀 업데이트 필요
- **가용성 위험**: 잘못된 핀 설정 시 서비스 중단
- **네트워크 의존**: DoH 서비스 가용성에 의존
- **유지보수**: 정기적인 핀 업데이트 작업 필요

### 모범 사례
1. **백업 핀**: 여러 개의 유효한 핀 미리 준비
2. **자동화**: 스크립트를 통한 정기 업데이트
3. **모니터링**: 검증 실패 시 알림 설정
4. **문서화**: 인증서 갱신 절차 문서화
5. **테스트**: 스테이징 환경에서 먼저 검증
