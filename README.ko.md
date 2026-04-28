# CheatSheet

Windows용 단축키 오버레이 앱입니다. 현재 활성 창의 프로세스 이름을 기준으로 기본 단축키와 사용자 정의 단축키를 보여줍니다.

[English README](README.md)

## 주요 기능

- `Ctrl+Shift+Space`로 오버레이 표시/숨김
- 현재 활성 프로세스별 단축키 목록 표시
- Windows 트레이 메뉴에서 설정 열기/종료
- 테마, 투명도, 오버레이 단축키, 창 위치 설정 저장
- 앱 안에서 사용자 정의 단축키 추가/수정
- JSON/CSV 파일 가져오기
- `Customs` 디렉터리의 사용자 정의 파일은 시작 시 파일 목록만 읽고, 해당 프로세스 오버레이가 필요할 때 내용만 지연 로드

## 실행

```powershell
cargo run
```

릴리스 빌드는 다음 명령을 사용합니다.

```powershell
cargo build --release
```

## 테스트

```powershell
cargo test
```

포맷 확인:

```powershell
cargo fmt -- --check
```

## 사용자 정의 단축키

사용자 정의 단축키는 Windows 기준으로 다음 위치에 저장됩니다.

```text
%APPDATA%\CheatSheet\Customs
```

파일명은 프로세스 app id가 됩니다.

```text
Customs\code.json
Customs\chrome.json
```

각 파일은 단축키 배열 JSON입니다.

```json
[
  {
    "combo": "Ctrl+P",
    "action": "Open file by name",
    "group": "Navigation"
  }
]
```

`group`이 비어 있으면 `Custom` 그룹으로 처리됩니다.

## 가져오기 형식

CSV는 현재 활성 프로세스에 단축키를 추가합니다.

```csv
combo,action,group
Ctrl+P,Open file by name,Navigation
Ctrl+Shift+F,Search in files,Search
```

JSON은 단일 앱 배열 형식 또는 여러 앱을 담은 카탈로그 형식을 사용할 수 있습니다.

## 개발 메모

- Rust 2024 edition
- GUI: `eframe`/`egui`
- 전역 단축키: `global-hotkey`
- 트레이 아이콘: `tray-icon`
- Windows 활성 창 감지: `windows` crate
