---
name: 2026-10-11-setbusy-kind-renderjob
description: Caution record for a solved false case or recurring risk.
---

# 작업 시작 시 setBusy(kind)를 renderJob보다 먼저 불러야 진행 카드 제목이 새 작업을 따른다

- Date: 2026-10-11
- Kind: `caution`
- Source: issueops-docs io-21fd3b1e1f06 (galpi#12)
- Summary: AppController.begin()이 renderJob()을 setBusy(kind)보다 먼저 부르면 AppView.jobKind가 이전 작업 종류로 남아, 같은 #job-panel을 쓰는 새 작업(Drive 가져오기)의 제목과 가시성이 이전 작업 기준으로 그려진다.
- Context: galpi#12 Drive 가져오기는 전사와 같은 #job-panel 진행 카드를 쓴다. 템플릿의 기본 제목이 '회의를 전사하고 있습니다'라서 전사 작업에서는 문제가 드러나지 않았고, 계획 6차 리뷰는 '다음 전사 시작 직후 제목이 잠깐 틀림'을 비차단으로 남겼다.
- Resolution: begin()에서 setBusy(kind)로 jobKind를 먼저 정한 뒤 renderJob(this.job)을 부른다. setBusy(null)에서 jobKind를 비우지는 않는다(취소 제목이 깨진다). src/ui/drive.dom.test.ts의 'a pending import shows a running card…' 테스트가 #job-title을 고정한다. 같은 카드를 쓰는 작업 종류를 추가할 때는 app-view.ts의 usesJobPanel()과 renderJobTitle() 분기를 함께 고친다.
- Evidence:
  - src/ui/controller.ts begin()
  - src/ui/app-view.ts usesJobPanel, renderJobTitle, driveImportTitles
  - src/ui/drive.dom.test.ts
