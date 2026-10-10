# 계획 검토 기록

## 1차: 수정 요청

- 로컬 원본의 회의 키를 출력 폴더로 정해, 같은 로컬 오디오를 다시 전사하면 출력 폴더가 '{stem} 2'로 바뀌어 Drive에 새 회의 폴더가 생기는 결함을 지적했다. 원본 경로 기반 키로 바꾸도록 요구했다.
- 브라우저 대기 전에 받은 gateway bearer가 최대 5분 대기 중 만료돼 교환이 401로 실패하는 결함을 지적했다.
- Drive에서 가져온 뒤 마이크 녹음을 전사하면 남은 driveSourceId 때문에 DRIVE_SOURCE_MISMATCH로 전사가 막히는 회귀를 지적했다.
- 호스트 bearer()가 gateway 거부로 세션을 지워도 화면은 로그인 상태로 남는 불일치를 지적했다.
- 가져온 녹음의 수정 시각이 가져온 시각이라 회의록 날짜가 틀어지는 결함을 지적했다.
- 연결 대기를 취소할 UI가 없고, Picker 대기 중 #cancel-button이 JOB_NOT_FOUND로 실패하는 결함을 지적했다.
- 증강 뒤 동기화에 증강 작업 ID를 넘기면 ARTIFACT_NOT_FOUND가 되는 모호함을 지적했다.
- 연결 해제와 진행 중 저장소 쓰기의 경합으로 토큰이 되살아날 수 있는 결함을 지적했다.

## 2차: 수정 요청

- 2차 델타 리뷰: 1라운드 지적 8건 중 6건(회의 키, bearer 시점, driveSourceId, 수정 시각, 증강 뒤 동기화 ID, 해제 경합)은 해결됐다고 확인했다.
- AUTH_REQUIRED 뒤 AccessController.check()는 signedIn 상태에서 아무 일도 하지 않아 로그인 화면으로 돌아가지 않는다고 지적했다(access-machine에 sessionExpired 전이 필요).
- 가져오기 중 #cancel-button이 가져오기 작업 ID와 busy 상태에 연결되는 계약이 없어 버튼이 숨거나 JOB_NOT_FOUND가 난다고 지적했다.
- #drive-cancel-button을 두 곳에 같은 id로 두면 설정에서 연결할 때 취소 버튼이 나타나지 않는다고 지적했다.

## 3차: 수정 요청

- 3차 델타 리뷰(anthropic/claude-opus-5-5, xhigh, 새 omp 세션): AUTH_REQUIRED 복귀(sessionExpired)와 취소 버튼 id 분리는 고쳐졌다고 확인했다.
- 가져오기 중 #cancel-button은 부모 #job-panel이 driveImport에서 숨겨지고 data-status가 completed로 남아 실제로 보이지 않는다고 지적했다.
- 다른 작업 실행 중에 Drive 가져오기를 누르면 begin이 실행 중 작업의 jobId와 busy 상태를 덮어써 취소가 JOB_NOT_FOUND로 끝나는 새 퇴행을 지적했다(가져오기 버튼을 busy·녹음 중 비활성으로).

## 4차: 수정 요청

- 4차 델타 리뷰(anthropic/claude-opus-5-5, xhigh, 새 omp 세션): 가져오기 중 패널·취소 버튼 표시(A)와 busy 중 가져오기 버튼 비활성(B)은 고쳐졌다고 확인했다.
- 가져오기가 끝나도 completeJob·handleFailure가 없어 #job-panel이 running으로 남고 실패 이유가 패널에 표시되지 않으며 제목이 전사 문구로 남는 새 회귀를 지적했다.

## 5차: 수정 요청

- 5차 델타 리뷰(anthropic/claude-opus-5-5, xhigh, 새 omp 세션): 가져오기 실패·취소 정리와 busy 해제, Drive 제목 분기는 고쳐졌다고 확인했다.
- 성공 시 completeJob을 쓰면 완료 CSS가 제목을 숨기고 '전사 완료' 요약 행을 보여 주며, completeJob이 TranscriptionResult를 받아 tsc가 실패한다고 지적했다(성공 시 패널을 숨기는 방식 요구).

## 6차: 통과

- 6차 델타 리뷰(anthropic/claude-opus-5-5, xhigh, 새 omp 세션; 5라운드 안에 끝나지 않아 5차 지적 2건을 반영한 뒤 한 번 더 독립 검토함): 가져오기 성공 시 completeJob 대신 initialJobState로 되돌리고 renderDriveImported가 패널을 숨겨 '전사 완료'가 보이지 않으며, tsc 타입 불일치도 없어졌다고 판정했다.
- begin·renderJob·setBusy 순서, 이벤트 필터, 취소 경쟁을 코드로 확인해 새 필수 결함이 없다고 판정했다. 비차단 메모: Drive 가져오기 성공 때 setAudio(path)를 renderDriveImported 앞에서 함께 부를 것, 남은 jobKind 때문에 다음 전사 시작 직후 제목이 잠깐 틀릴 수 있음(기존 코드와 같은 종류).
