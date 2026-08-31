<p align="center">
  <img src="./docs/assets/branding/coding-harness-manager-hero.svg" width="760" alt="하나의 구성 라이브러리를 여러 코딩 하네스에 동기화하는 Coding Harness Manager"/>
</p>

<h1 align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="./resources/logos/logo-dark.svg"/>
    <img src="./resources/logos/logo-light.svg" width="660" alt="Coding Harness Manager"/>
  </picture>
</h1>

<p align="center">
  <a href="./README.md">English</a> · <strong>한국어</strong>
</p>

<p align="center">
  <a href="./LICENSE"><img src="https://img.shields.io/badge/License-Patty--Public--1.0-1769e0.svg?style=flat-square&labelColor=161616" alt="Patty Public License 1.0"/></a>
  <a href="./.github/workflows/ci.yml"><img src="https://img.shields.io/badge/CI-GitHub--Actions-1769e0.svg?style=flat-square&labelColor=161616" alt="GitHub Actions CI"/></a>
  <img src="https://img.shields.io/badge/Desktop-Tauri--2-1769e0.svg?style=flat-square&labelColor=161616" alt="Tauri 2 데스크톱 애플리케이션"/>
  <a href="https://patty.io"><img src="https://img.shields.io/badge/PATTY.IO-patty.io-1769e0.svg?style=flat-square&labelColor=161616" alt="patty.io"/></a>
</p>

<h3 align="center">한 번 구성하고, 변경점을 확인한 뒤, 어디든 동기화하세요.</h3>

<p align="center">
  Claude Code, Codex, OpenCode, Pi, Reasonix와 여러 코딩 하네스에 흩어진<br/>
  공급자, 모델, MCP 서버, 스킬, 실행 프로필을 관리하는 데스크톱 컨트롤 플레인입니다.
</p>

코딩 하네스는 강력하지만 도구마다 공급자 설정, 모델 별칭, MCP 정의,
스킬, 구성 파일 형식이 모두 다릅니다. Coding Harness Manager(CHM)는 각
도구의 네이티브 파일을 감추거나 대체하지 않으면서 공용 라이브러리를
제공합니다.

이미 설치된 도구를 스캔하고, 유지할 설정을 가져오고, 네이티브 diff를
미리 확인한 다음 승인한 변경만 적용할 수 있습니다. 모든 쓰기는 백업과
기록을 남기므로 이후에 변경 내용을 이해하고, 수용하거나, 롤백할 수
있습니다.

> [!NOTE]
> CHM은 현재 1.0 이전 버전입니다. 어댑터 기능은 하네스와 구성 영역별로
> 표시되며, 지원 여부를 하나의 전체 지원/미지원 상태로 취급하지 않습니다.

## 왜 필요한가요?

| CHM 없이 | CHM을 사용하면 |
|---|---|
| 여러 도구에 같은 공급자와 모델 설정을 반복 | 재사용 가능한 공급자, 엔드포인트, 모델 경로를 한 라이브러리에서 관리 |
| 어떤 모델 별칭이 어느 게이트웨이에 속하는지 기억 | 공급자 식별자, 엔드포인트 URL, 원격 모델 ID를 분리해 보존 |
| 서로 다른 형식에 MCP 서버와 스킬을 수동 복사 | 하네스별 어댑터로 감지, 그룹화, 가져오기, 동기화 |
| JSON, JSONC, TOML, YAML을 직접 편집 | 라이브러리 동기화 전에 네이티브 변경점을 미리 확인 |
| 문제가 생긴 뒤 외부 구성 변경을 뒤늦게 발견 | 강조 표시된 drift를 보고 수용하거나 마지막 CHM 기준으로 복원 |
| 직접 백업 기록을 유지 | 원자적 쓰기, 백업, 스냅샷, 설명형 기록, 롤백 사용 |

## CHM이 관리하는 항목

- **하네스 인벤토리** — 실행 파일, 버전, 구성 경로, 모델, MCP 서버,
  스킬, 쓰기 가능한 기능, 외부 변경 상태.
- **공급자와 엔드포인트** — 공급자 식별자를 프로토콜, 기본 URL, 검색
  경로, 인증 방식, 자격 증명 참조와 분리해 관리합니다.
- **My Models** — 표시 이름, 컨텍스트 윈도우, 최대 입력, 최대 출력,
  공급자, 엔드포인트 메타데이터를 가진 재사용 가능한 모델 경로입니다.
- **MCP 서버와 스킬** — 여러 하네스에서 감지하고 논리적으로 같은 항목을
  그룹화하며, 지원하는 형식에만 동기화합니다.
- **프로필과 세트** — 선택한 모델로 하네스를 실행하거나 모델, MCP 서버,
  스킬 묶음을 미리 확인하고 적용합니다.
- **기록과 안전 장치** — 계획, diff, 백업, 스냅샷, 외부 변경 처리,
  롤백, Doctor 진단, 비밀값이 제거된 내보내기.

## 지원하는 하네스

| 어댑터 그룹 | 하네스 |
|---|---|
| 주요 어댑터 | Claude Code, Codex, OpenCode, Pi, Reasonix |
| 추가 형식 인식 어댑터 | Gemini CLI, Qwen Code, Kimi CLI, Cursor, Cline, Roo Code, Aider, Amp, Goose, Continue |

각 어댑터는 해당 하네스가 실제로 저장하는 네이티브 구성 영역만
보고합니다. 예를 들어 어떤 하네스는 모델 레지스트리를 제공하지만 다른
하네스는 모델 선택, MCP 또는 스킬만 제공할 수 있습니다. CHM은 이러한
기능 차이를 앱에 표시하며 지원하지 않는 쓰기를 만들어내지 않습니다.

## 작동 방식

```text
네이티브 하네스 구성
        │
        ▼
  스캔 + 읽기 전용 가져오기 ──────► CHM 라이브러리
                                         │
                                         ▼
                                  원하는 상태 vs 실제 상태
                                         │
                                         ▼
                                  계획 + 네이티브 diff
                                         │
                                   명시적인 Apply만 실행
                                         │
                                         ▼
                              원자적 쓰기 + 백업 + 검증
                                         │
                                         ▼
                                    History / 롤백
```

기존 설정 가져오기는 하네스 파일을 읽지만 변경하지 않습니다. 라이브러리
동기화는 **Desired → Plan → Preview → Apply → Verify** 순서를 따릅니다.
하네스 직접 편집은 별도로 확인하는 작업이며 동일하게 백업과 History
스냅샷을 생성합니다.

## 설치

### 설치 파일 다운로드

저장소의 **Releases** 탭에서 플랫폼에 맞게 게시된 패키지를
다운로드하세요.

| 플랫폼 | 패키지 |
|---|---|
| macOS Apple Silicon | `.dmg` / `.app` (`aarch64`) |
| macOS Intel | `.dmg` / `.app` (`x86_64`) |
| Windows | `.msi` 또는 NSIS `.exe` |
| Linux | `.AppImage` 또는 `.deb` |

macOS 서명되지 않은 빌드 안내를 포함한 플랫폼별 설명은
[설치 가이드](./docs/installation.md)를 참고하세요.

### 소스에서 실행

Rust stable 1.85 이상, Node.js 22, npm 11 이상과 운영체제에 맞는
[Tauri 2 플랫폼 의존성](./docs/development.md)이 필요합니다.

```bash
npm ci --prefix apps/desktop
npm run tauri dev --prefix apps/desktop
```

CHM은 첫 실행 시 로컬 레지스트리를
`~/.coding-harness-manager/chm.sqlite`에 생성합니다.

## 5분 빠른 시작

1. **Harnesses**를 열고 **Scan machine**을 클릭합니다.
2. **Import existing setup**을 열어 선택한 하네스마다 감지된 공급자,
   모델, MCP 서버, 스킬을 검토합니다. 이 가져오기는 하네스 파일에 쓰지
   않습니다.
3. **Providers**에서 엔드포인트를 추가하거나 확인하고 자격 증명을
   설정합니다.
4. **Discover models**를 클릭한 뒤 사용할 모델을 **My Models**에
   추가합니다.
5. 하네스를 열고 **Sync from library…**를 선택합니다.
6. 원하는 모델을 선택하고 계획과 네이티브 diff를 확인한 다음
   **Apply**를 클릭합니다.
7. 트랜잭션을 확인하거나 롤백하려면 **History**를 사용합니다.

## 기존 설정을 스캔하고 가져오는 방법

1. **Harnesses → Scan machine**으로 이동합니다.
2. 새 CLI를 설치하거나 구성 위치를 변경했다면 다시 스캔하세요. 데스크톱
   앱은 이전 인벤토리가 계속 최신이라고 가정하지 않습니다.
3. **Import existing setup**을 엽니다.
4. 검토할 하네스를 선택합니다. CHM은 가져오기 전에 감지된 공급자, 모델,
   MCP 서버, 스킬을 보여줍니다.
5. 중앙 라이브러리에 넣을 리소스를 확인합니다.

가져오기 마법사는 CHM 레지스트리만 갱신하며 원본 하네스를 다시 쓰지
않습니다. 같은 리소스가 이미 있으면 조용히 덮어쓰지 않고 중복으로
보고합니다.

## 공급자를 추가하고 모델을 검색하는 방법

1. **Providers → Add provider**를 열고 공급자에 안정적인 이름을
   지정합니다.
2. 올바른 프로토콜, 기본 URL, 인증 방식, 검색 경로로 엔드포인트를
   추가합니다. 일반적인 검색 경로는 `/v1/models`와 `/models`입니다.
3. 자격 증명 소스를 선택합니다.
   - **macOS Keychain**은 비밀값을 `coding-harness-manager` 서비스에
     저장합니다.
   - **Environment variable**은 변수 이름만 저장합니다. CHM을 시작하기
     전에 해당 변수를 export하세요.
4. 엔드포인트 상태 확인을 실행하고 **Discover models**를 클릭합니다.
5. 원하는 카탈로그 항목을 선택하고 **Add to My Models**를 누릅니다.

하네스에서 가져온 공급자는 이름, 그룹, 기본 URL을 CHM으로 가져올 수
있지만 하네스가 노출하지 않는 비밀값은 복사할 수 없습니다. 모델 검색
전에 API 키를 별도로 추가하거나 참조하세요. 공급자 모델 카탈로그는 여러
엔드포인트에서 중복을 제거해 표시합니다.

> [!IMPORTANT]
> 현재 macOS Keychain과 환경 변수 자격 증명을 사용할 수 있습니다.
> Windows Credential Manager와 Linux libsecret 백엔드는 플랫폼 훅만
> 존재하고 아직 완성되지 않았습니다. 해당 플랫폼에서는 현재 환경 변수
> 참조를 사용하세요.

## 모델 메타데이터를 관리하는 방법

하네스에 동기화할 수 있는 경로는 **My Models**에서 관리합니다.

- **Edit**에서 표시 이름, 컨텍스트 윈도우, 최대 입력, 최대 출력을 직접
  설정합니다.
- **Match metadata**는 원격 모델 ID를 CHM에 번들된 `models.dev`
  카탈로그와 비교합니다. 선택 기능이며 공급자에게 요청하지 않고, 비어
  있는 메타데이터만 채우며, 결과가 없거나 모호할 수 있습니다.
- **Discovered** 탭에서 공급자 카탈로그 모델을 일괄 추가합니다.
- 같은 원격 모델 ID가 여러 게이트웨이에 있을 때 공급자 또는
  엔드포인트로 필터링합니다.

경로 식별자는 `(endpoint_id, remote_model_id)`입니다. 표시 이름은
사람을 위한 값이며 동기화에 사용하는 엔드포인트나 원격 모델 ID를
대체하지 않습니다.

## 모델을 하네스에 동기화하는 방법

1. **Harnesses**를 열고 대상 하네스를 선택합니다.
2. **Sync from library…**를 클릭합니다.
3. 이 하네스에 사용할 라이브러리 모델을 선택합니다.
4. 추가, 업데이트, 제거, 충돌, 미지원 항목을 검토합니다.
5. 네이티브 구성 diff를 확인합니다.
6. 계획이 맞을 때만 **Apply**를 클릭합니다.

미리보기는 현재 파일 상태와 연결됩니다. Apply 전에 파일이 바뀌면 CHM은
오래된 계획을 쓰지 않고 해당 미리보기를 거부합니다.

방향을 나타내는 버튼 문구는 의도적으로 구분되어 있습니다.

- **Import from library…**는 라이브러리 → 이 하네스를 뜻합니다.
- **To library**는 이 하네스의 로컬 모델 → My Models를 뜻합니다.

## 하네스에 이미 있는 모델을 관리하는 방법

하네스 상세 화면은 네이티브 ID, 원격 모델 ID, 표시 이름, 연결된 공급자,
컨텍스트 윈도우, 라이브러리 상태 등 현재 디스크에 실제로 있는 내용을
보여줍니다.

- **공급자 이름**을 클릭하면 상세 정보를 엽니다. 공급자가 하네스 구성에만
  있으면 CHM이 선언된 이름과 기본 URL로 공급자와 엔드포인트 레지스트리
  항목을 만들 수 있습니다.
- **To library**로 해당 경로만 중앙에서 관리합니다.
- **Edit**로 쓰기 가능한 네이티브 필드를 수정합니다.
- **Duplicate**로 원본 모델을 확인하고 고유한 새 모델 ID와 표시 이름을
  지정합니다.
- **Delete**로 제거를 확인합니다. CHM은 구성을 백업하고 정확한
  모델/공급자 변경 내용을 History에 기록합니다.

직접 편집 가능 여부는 어댑터와 감지된 하네스 버전에 따라 달라집니다.

## CHM 외부에서 바뀐 구성을 처리하는 방법

CHM이 마지막으로 확인하거나 쓴 뒤 하네스 파일이 변경되었다면 다음과
같이 처리합니다.

1. 해당 하네스를 열고 **Show diff**를 선택합니다.
2. 강조 표시된 추가와 제거를 검토합니다. **Previous**와 **Next**로 변경
   그룹 사이를 이동합니다.
3. 명확한 두 결과 중 하나를 선택합니다.
   - **Accept local changes** — 현재 파일을 유지하고 새 CHM 기준으로
     기록합니다.
   - **Revert to last app baseline** — CHM이 마지막으로 쓴 버전을
     복원합니다.

Revert는 먼저 현재 로컬 파일을 백업하므로 되돌리기 작업 자체도
History에서 다시 취소할 수 있습니다.

## MCP 서버와 스킬을 관리하는 방법

### MCP 서버

1. **MCP Servers**에서 라이브러리 항목과 감지된 구성을 확인합니다.
2. CHM은 논리적 서버 이름으로 감지 결과를 그룹화하면서 서로 다른 전송
   방식, 명령, URL, 출처 하네스를 모두 보존합니다.
3. 구성 상세를 검토하고 서버를 라이브러리에 한 번만 추가합니다.
4. **Sync to harness**를 사용하고 적용 전에 대상의 네이티브 계획을
   검토합니다.

### 스킬

1. **Skills**를 열고 지원하는 하네스 폴더에서 사용 가능한 스킬을
   감지합니다.
2. 원하는 스킬을 표준 라이브러리 복사본으로 가져옵니다.
3. 호환 가능한 스킬 영역을 제공하는 하네스에만 동기화하거나
   바인딩합니다.

## 프로필과 세트를 사용하는 방법

- **Profiles**는 하네스와 선택한 모델/엔드포인트 구성을 연결해 반복 가능한
  코딩 환경을 실행합니다.
- **Sets**는 재사용 가능한 모델, MCP 서버, 스킬을 묶습니다. 대상 하네스에
  대한 세트를 미리 확인하고 차단 항목을 해결한 다음 쓰기 가능한 변경을
  함께 적용합니다.

## 백업, 복원, 진단 방법

**Settings**에는 서로 다른 두 가지 이동/보호 도구가 있습니다.

- **데이터베이스 백업과 복원**은 로컬 CHM 레지스트리를 보호합니다.
- **이동 가능한 구성 내보내기/가져오기**는 공급자, 엔드포인트, 모델,
  MCP 서버, 스킬, 프로필, 세트를 옮기되 비밀값을 읽거나 내보내지
  않습니다.

**Doctor**는 하네스, 공급자, MCP, 스킬에 대한 읽기 전용 검사를
실행합니다. 진단 내보내기는 비밀값을 제거하며, 검토 후 버그 보고서에
첨부할 수 있습니다.

## 안전 모델

- 하네스 가져오기는 읽기 전용입니다.
- 라이브러리 동기화는 Apply 전에 항상 계획과 미리보기를 만듭니다.
- 직접 실행하는 파괴적 작업은 앱 내부 확인 대화상자를 요구합니다.
- 네이티브 쓰기는 원자적으로 수행되며 백업과 트랜잭션 스냅샷을 만듭니다.
- 알 수 없거나 지원하지 않는 어댑터 기능에 임의의 쓰기를 하지 않습니다.
- SQLite에는 API 키 값이 아닌 자격 증명 참조만 저장합니다.
- 이동 가능한 내보내기와 Doctor 진단은 비밀값을 제거합니다.
- CHM은 관리하는 네이티브 하위 영역만 변경하고 나머지 구성을 보존합니다.

## 문제 해결

### 하네스가 없거나 이전 상태로 계속 표시됩니다

GUI의 `PATH`가 셸과 다를 수 있습니다. 하네스를 설치하고 사용자 구성이
있는지 확인한 뒤 **Scan machine**을 다시 클릭하세요. 실행 파일과 구성
경로 대체 규칙은 [하네스 감지 문서](./docs/harnesses/detection.md)를
참고하세요.

### 모델 검색에서 인증 실패가 표시됩니다

엔드포인트 인증 방식, 기본 URL, 자격 증명 참조를 확인하세요. Keychain
자격 증명은 공급자 화면에서 키를 다시 저장하고, 환경 변수 자격 증명은
CHM을 실행하기 전에 해당 변수를 export하세요.

### 모델 검색에서 잘못된 응답이 표시됩니다

엔드포인트 응답에 예상한 모델 목록이 없다는 뜻입니다. 프로토콜과 검색
경로를 확인하고 URL이 HTML 페이지나 API 오류 객체를 반환하지 않는지
확인하세요.

### 가져온 모델에 컨텍스트 윈도우가 없습니다

**My Models → Edit**에서 직접 설정하거나 번들 카탈로그에 대해
**Match metadata**를 실행하세요. 공급자 검색 응답이 컨텍스트 또는 출력
제한을 항상 포함하는 것은 아닙니다.

### 쓰기가 실패했거나 예상과 다른 결과가 나왔습니다

오류를 닫지 말고 **History**에서 트랜잭션과 파일 스냅샷을 확인하세요.
필요하면 History에서 롤백합니다.
`~/.coding-harness-manager/chm.sqlite`를 직접 편집하지 마세요.

## 개발

```bash
# 공용 Rust 워크스페이스
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace

# Tauri 백엔드(별도 manifest)
cargo check --manifest-path apps/desktop/src-tauri/Cargo.toml
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml

# 프론트엔드
npm ci --prefix apps/desktop
npm run lint --prefix apps/desktop
npm test --prefix apps/desktop
npm run build --prefix apps/desktop
```

어댑터를 기여하거나 네이티브 구성 writer를 변경하기 전에
[CONTRIBUTING.md](./CONTRIBUTING.md)와
[개발 가이드](./docs/development.md)를 확인하세요.

## 문서

- [설치 가이드](./docs/installation.md)
- [하네스 형식 및 감지 조사](./docs/harnesses/)
- [개발 가이드](./docs/development.md)
- [기여 안내](./CONTRIBUTING.md)

## 라이선스

Coding Harness Manager는 **[Patty Public License 1.0](./LICENSE)**으로
배포됩니다. Apache License 2.0을 기반으로 하며, 평균 연간 매출이 미화
1억 달러 이상인 조직에는 별도의 상용 라이선스를 요구하는 조항이
추가되어 있습니다. 개인, 스타트업, 학계, 비영리 단체, 평가, 연구,
기여는 라이선스 조건에 따라 계속 허용됩니다.

상용 라이선스 문의: [licensing@patty.io](mailto:licensing@patty.io)

<p align="center">
  <strong>Patty Coding Harness Manager</strong><br/>
  <sub>하나의 라이브러리 · 네이티브 형식 · 되돌릴 수 있는 변경</sub>
</p>
