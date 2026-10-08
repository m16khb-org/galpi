# 요청자 의도 계약

- lifecycle: io-0c055863a1cf
- issue: https://github.com/m16khb-org/galpi/issues/5
- intent_class: standard

## 원문 요청
windows도 지원해야하고 codex sign in도 지원해야 하고 당근 디자인 시스템도 적용해야해 로 각각 이슈부터 만들자 / (추가 지시) 디자인 시스템만 당근걸 쓰는거지 디자인을 바꾸겠다는거는아니야

## 해석
갈피 프런트엔드의 손으로 만든 디자인 토큰(src/styles.css의 --surface-*/--text-*/--accent-*/--status-*/--space-*)과 공통 컴포넌트 스타일(버튼, 입력, 셀렉트, 세그먼트 컨트롤, 토글, 시트)을 당근 SEED Design System의 프레임워크 중립 패키지 @seed-design/css(base.css 토큰 + recipes)로 교체한다. 화면 구성·레이아웃·흐름·카피·상태 모델·접근성 계약은 그대로 유지한다(리디자인 아님). React는 도입하지 않는다. 루트 DESIGN.md는 SEED 토큰을 참조하도록 갱신한다.

## 성공 기준
- 1) package.json에 @seed-design/css가 추가되고 src/styles.css가 base.css를 import해 색·간격·타이포·모션 값을 --seed-* 변수에서 받는다(갈피 고유 hex/px 토큰 정의 0개). 2) 버튼·텍스트 필드·셀렉트·세그먼트 컨트롤·토글·시트가 SEED recipe 클래스로 렌더되고 idle/loading/success/error/disabled 상태 라벨이 그대로 동작한다. 3) app-template.ts의 DOM 구조와 required selector가 유지되어 기존 dom 테스트(bun test)가 통과한다. 4) 레이아웃(레일+메인+보조 패널), 파형 진행 룰, 상태 색+텍스트 쌍, 40px 터치 타깃, keep-all 줄바꿈, reduced-motion 동작이 유지된다. 5) 루트 DESIGN.md의 팔레트·타이포·간격 표가 SEED 토큰 이름으로 갱신되고 .issueops/DESIGN.md가 이를 가리킨다. 6) bun run check와 bun run build가 통과한다.

## 비목표
- (없음)

## 제약
- (없음)

## 모호함
- (없음)

## 읽는 규칙
이 문서는 요청자 의도 계약이다. 원격 이슈 본문은 구현 계약이다. 두 문서가 충돌하면 구현을 시작하지 말고 충돌한 줄을 인용해 blocker로 보고한다.
