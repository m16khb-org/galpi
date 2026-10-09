import { version } from "../../package.json"
import appIconUrl from "../../assets/app-icon.svg"
import { actionButtonClass } from "./seed"

const primary = actionButtonClass("brandSolid")
const secondary = actionButtonClass("neutralOutline")
const ghost = actionButtonClass("ghost")
const iconOutline = actionButtonClass("neutralOutline", "medium", "iconOnly")
const recordAction = actionButtonClass("neutralOutline", "xsmall")
const dismiss = actionButtonClass("ghost", "xsmall", "iconOnly")

export const appTemplate = `
  <div class="app-shell">
    <aside class="setup-rail" aria-label="진행 단계">
      <div class="brand-lockup">
        <span class="brand-mark" aria-hidden="true"><img src="${appIconUrl}" alt="" /></span>
        <div><strong>갈피</strong><span>LOCAL TRANSCRIPTION</span></div>
      </div>
      <ol class="step-list">
        <li id="step-transcribe" data-state="current" aria-current="step"><span>01</span><div><strong>회의 전사</strong><small>오디오에서 결과까지</small><em class="step-state" data-step-state>현재 단계</em></div></li>
        <li id="step-results" data-state="pending"><span>02</span><div><strong>전사 결과</strong><small>자막 · 화자별 텍스트</small><em class="step-state" data-step-state>대기</em></div></li>
        <li id="step-augment" data-state="pending"><span>03</span><div><strong>전사 결과 AI 증강</strong><small>회의록 자동 작성</small><em class="step-state" data-step-state>대기</em></div></li>
      </ol>
      <div class="rail-note"><i class="ph ph-shield-check" aria-hidden="true"></i><p>녹음과 전사는 이 컴퓨터 안에서만 처리됩니다. AI 증강을 실행할 때만 전사본이 증강 제공자로 전송됩니다.</p></div>
    </aside>

    <main class="workspace">
      <header class="topbar">
        <div><span class="eyebrow">LOCAL AUDIO WORKSPACE</span><h1 id="task-title">새 회의 전사</h1><p id="task-status" class="task-status">로컬 환경을 확인하는 중입니다.</p></div>
        <div class="topbar-actions">
          <div class="engine-chip"><span id="setup-state" data-state="pending">확인 중</span><small id="engine-version">확인 중</small></div>
          <button class="settings-button ${iconOutline}" type="button" data-action="open-settings" aria-label="설정 열기"><i class="ph ph-gear" aria-hidden="true"></i></button>
        </div>
      </header>

      <p id="app-error" class="app-error" role="alert" hidden></p>

      <div class="workspace-body">
        <section id="setup-panel" class="panel setup-panel" aria-labelledby="setup-title">
          <div class="section-heading">
            <div><span class="section-index">00 / 준비</span><h2 id="setup-title">로컬 AI 환경</h2></div>
            <p>Galpi가 앱 전용 Python 런타임과 선택한 전사 엔진을 설치하고 필요한 모델을 한 번에 준비합니다. 엔진은 설정의 <strong>전사 엔진</strong>에서 바꿀 수 있습니다.</p>
          </div>
          <div class="setup-grid">
            <div class="status-list" aria-label="설치 상태">
              <div id="engine-check" class="status-row" data-state="pending"><i class="ph ph-cpu" aria-hidden="true"></i><span data-status-label>엔진</span><strong data-status-value>확인 중</strong></div>
              <div id="model-check" class="status-row" data-state="pending"><i class="ph ph-brain" aria-hidden="true"></i><span data-status-label>전사 모델</span><strong data-status-value>확인 중</strong></div>
              <div id="ffmpeg-check" class="status-row" data-state="pending"><i class="ph ph-waveform" aria-hidden="true"></i><span data-status-label>내장 ffmpeg</span><strong data-status-value>확인 중</strong></div>
            </div>
            <div class="token-field token-summary">
              <div class="token-summary-header"><strong>Hugging Face 토큰</strong><span id="token-configured-state">확인 중</span></div>
              <p>화자분리 모델 접근 토큰은 우측 상단 설정에서 저장하고 다시 확인할 수 있습니다.</p>
              <button class="text-button ${ghost}" type="button" data-action="open-settings">토큰 설정 열기 <i class="ph ph-gear" aria-hidden="true"></i></button>
            </div>
          </div>
          <div id="setup-progress-panel" class="setup-progress-card" hidden>
            <div class="job-header"><div><span class="section-index">앱 전용 환경 설치</span><h3 id="setup-job-title">로컬 환경을 준비하고 있습니다</h3></div><strong id="setup-job-percent">0%</strong></div>
            <div id="setup-job-progress" class="wave-progress" role="progressbar" aria-label="로컬 환경 준비 진행률" aria-valuemin="0" aria-valuemax="100" aria-valuenow="0"><span></span></div>
            <ol class="phase-list setup-phase-list">
              <li data-setup-phase="engine" data-state="pending">Python 런타임</li>
              <li data-setup-phase="models" data-state="pending">모델 · ffmpeg</li>
            </ol>
            <p id="setup-job-message" class="job-message" aria-live="polite"></p>
            <p id="setup-error-message" class="error-message" role="alert" hidden></p>
            <div class="job-actions">
              <button id="setup-cancel-button" class="secondary-button danger ${secondary}" type="button" data-action="cancel">설치 취소</button>
              <details><summary>설치 상세 로그</summary><pre id="setup-log-output"></pre></details>
            </div>
          </div>
          <div class="panel-actions">
            <button id="prepare-button" class="primary-button ${primary}" type="button" data-action="prepare"><i class="ph ph-download-simple" aria-hidden="true"></i><span>로컬 엔진 준비</span></button>
            <span class="action-note">첫 실행은 약 3GB의 모델을 내려받을 수 있습니다.</span>
          </div>
        </section>

        <section id="transcription-panel" class="panel transcription-panel" aria-labelledby="transcription-title">
          <div class="section-heading">
            <div><span id="transcription-index" class="section-index">01 / 녹음·파일</span><h2 id="transcription-title">새 회의 전사</h2></div>
            <p>참석 인원을 알려주면 겹치는 목소리와 짧은 발화를 더 안정적으로 분리합니다.</p>
          </div>
          <div class="transcription-grid">
            <div class="primary-task">
              <div id="recorder" class="recorder" data-state="idle">
                <button id="record-button" class="record-button" type="button" data-action="record">
                  <span><i class="ph ph-microphone" aria-hidden="true"></i></span>
                  <div><strong>마이크로 바로 녹음</strong><small>시스템 마이크 · 16-bit PCM WAV</small></div>
                </button>
                <div id="recording-active" class="recording-active" hidden>
                  <span class="recording-dot" aria-hidden="true"></span>
                  <div><strong id="recording-label">녹음 중</strong><small id="recording-path">마이크 입력을 저장하고 있습니다.</small></div>
                  <time id="recording-time" datetime="PT0S">00:00</time>
                  <button id="stop-recording-button" class="record-stop ${recordAction}" type="button" data-action="stop-recording"><i class="ph ph-stop" aria-hidden="true"></i> 정지</button>
                  <button id="cancel-recording-button" class="record-discard ${recordAction}" type="button" data-action="cancel-recording">버리기</button>
                </div>
                <p id="recording-status" class="recording-status" aria-live="polite">마이크로 바로 녹음할 수 있습니다.</p>
              </div>
              <div class="choice-divider"><span>또는 기존 파일</span></div>
              <button id="audio-selection" class="file-picker" type="button" data-action="choose-audio" data-selected="false">
                <span class="file-icon"><i class="ph ph-music-notes" aria-hidden="true"></i></span>
                <span><strong>오디오 파일 선택</strong><small id="audio-path">m4a, mp3, wav, mp4, mov, flac</small></span>
                <i class="ph ph-caret-right" aria-hidden="true"></i>
              </button>
              <div class="field-block">
                <div class="field-label"><span>출력 폴더</span><button id="output-button" class="${ghost}" type="button" data-action="choose-output">변경</button></div>
                <div id="output-path" class="path-display">출력 폴더를 선택하세요.</div>
              </div>
            </div>
            <fieldset class="speaker-panel">
              <legend>참석자</legend>
              <div class="participant-picker">
                <div class="participant-picker-header">
                  <span id="attendee-count">0명 선택</span>
                  <button id="attendee-clear" class="text-button ${ghost}" type="button" data-action="clear-attendees" hidden>전체 해제</button>
                </div>
                <div id="attendee-chips" class="participant-chips" role="group" aria-label="참석자 선택"></div>
                <p id="attendee-empty" class="participant-empty">설정에서 참석자 명부를 만들면 회의마다 여기서 고를 수 있습니다. <button class="text-button ${ghost}" type="button" data-action="open-settings">명부 만들기</button></p>
              </div>
              <p class="speaker-hint-label">화자 수 힌트</p>
              <div class="segmented-control">
                <label><input type="radio" name="speaker-mode" value="auto" checked /><span>자동</span></label>
                <label><input type="radio" name="speaker-mode" value="exact" /><span>정확히</span></label>
                <label><input type="radio" name="speaker-mode" value="range" /><span>범위</span></label>
              </div>
              <p id="speaker-hint-note">참석 인원을 모르면 자동을 선택해도 됩니다.</p>
              <div id="exact-fields" class="number-fields" hidden><label for="exact-speakers">참석 인원</label><input id="exact-speakers" type="number" min="1" max="30" value="4" /></div>
              <div id="range-fields" class="number-fields range" hidden><label for="min-speakers">최소</label><input id="min-speakers" type="number" min="1" max="30" value="3" /><span>–</span><label for="max-speakers">최대</label><input id="max-speakers" type="number" min="1" max="30" value="7" /></div>
            </fieldset>
          </div>
          <div id="job-panel" class="setup-progress-card" data-status="idle" hidden>
            <p class="job-summary"><i class="ph ph-check" aria-hidden="true"></i><span id="job-summary-text">전사 완료</span></p>
            <div class="job-header"><div><span id="busy-label" class="section-index"></span><h3 id="job-title">회의를 전사하고 있습니다</h3></div><strong id="job-percent">0%</strong></div>
            <div id="job-progress" class="wave-progress" role="progressbar" aria-label="현재 단계 진행률" aria-valuemin="0" aria-valuemax="100" aria-valuenow="0"><span></span></div>
            <ol id="job-phase-list" class="phase-list">
              <li data-phase="transcribing" data-state="pending">전사</li>
              <li data-phase="aligning" data-state="pending">정렬</li>
              <li data-phase="diarizing" data-state="pending">화자분리</li>
              <li data-phase="writing" data-state="pending">결과 저장</li>
            </ol>
            <p id="job-message" class="job-message" aria-live="polite"></p>
            <p id="error-message" class="error-message" role="alert" hidden></p>
            <div class="job-actions">
              <button id="cancel-button" class="secondary-button danger ${secondary}" type="button" data-action="cancel" hidden>작업 취소</button>
              <details><summary>상세 로그</summary><pre id="log-output"></pre></details>
            </div>
          </div>
          <div class="panel-actions">
            <button id="start-button" class="primary-button ${primary}" type="button" data-action="transcribe" disabled><i class="ph ph-play" aria-hidden="true"></i><span id="start-label">전사 시작</span></button>
            <span class="action-note">체크포인트가 있으면 전사·정렬을 재사용합니다.</span>
          </div>
        </section>

        <section id="results-panel" class="panel results-panel" hidden aria-labelledby="results-title">
          <div class="section-heading"><div><span id="results-index" class="section-index">02 / 산출물</span><h2 id="results-title">전사 결과</h2></div><p id="result-summary"></p></div>
          <div class="artifact-list">
            <div id="result-srt-row" class="artifact-row"><i class="ph ph-subtitles" aria-hidden="true"></i><div><strong>자막 파일</strong><code id="result-srt"></code></div><button class="${ghost}" type="button" data-action="open-srt" aria-label="자막 파일 열기">열기</button></div>
            <div class="artifact-row"><i class="ph ph-users-three" aria-hidden="true"></i><div><strong>화자별 텍스트</strong><code id="result-txt"></code></div><button class="${ghost}" type="button" data-action="open-txt" aria-label="화자별 텍스트 열기">열기</button></div>
            <div id="result-checkpoint-row" class="artifact-row"><i class="ph ph-database" aria-hidden="true"></i><div><strong>정렬 체크포인트</strong><code id="result-checkpoint"></code></div><button class="${ghost}" type="button" data-action="open-checkpoint" aria-label="정렬 체크포인트 열기">열기</button></div>
          </div>
          <div class="panel-actions">
            <button class="secondary-button ${secondary}" type="button" data-action="reveal-output"><i class="ph ph-folder-open" aria-hidden="true"></i>출력 폴더 열기</button>
          </div>
        </section>

        <section id="augment-panel" class="panel augment-panel" aria-labelledby="augment-title">
          <div class="section-heading">
            <div><span class="section-index">03 / AI 증강</span><h2 id="augment-title">회의록 작성</h2></div>
            <p>등록한 OpenAI 호환 API 토큰으로 전사 결과를 회의록으로 정리합니다. 결정사항과 실행·추적 항목을 놓치지 않습니다.</p>
          </div>
          <div id="augment-key-hint" class="augment-hint" hidden>
            <i class="ph ph-key" aria-hidden="true"></i>
            <p>AI 증강에는 OpenAI 호환 API 키 또는 ChatGPT 로그인이 필요합니다. <button class="text-button ${ghost}" type="button" data-action="open-settings">설정에서 등록</button></p>
          </div>
          <div id="chatgpt-limit-hint" class="augment-hint chatgpt-limit-hint" role="alert" hidden>
            <i class="ph ph-info" aria-hidden="true"></i>
            <div>
              <p>ChatGPT 사용량 한도에 도달해 회의록을 만들지 못했습니다. 한도가 초기화된 뒤 다시 실행해 주세요. 앱은 자동으로 다시 시도하지 않습니다.</p>
              <button class="primary-button ${primary}" type="button" data-action="open-chatgpt-usage">사용량 관리</button>
            </div>
          </div>
          <p id="augment-waiting" class="augment-hint"><i class="ph ph-hourglass" aria-hidden="true"></i>전사를 마치거나 전사문을 가져오면 이 단계에서 바로 회의록을 증강할 수 있습니다.</p>
          <button id="transcript-selection" class="file-picker" type="button" data-action="import-transcript" data-selected="false">
            <span class="file-icon"><i class="ph ph-file-text" aria-hidden="true"></i></span>
            <span><strong id="transcript-title">전사문 파일 가져오기</strong><small id="transcript-path">txt, md</small></span>
            <span class="picker-cta">다른 파일 가져오기</span>
            <i class="ph ph-caret-right" aria-hidden="true"></i>
          </button>
          <div id="augment-progress" class="setup-progress-card" hidden>
            <div class="job-header"><div><span class="section-index">AI 증강 진행 중</span><h3 id="augment-job-title">회의록을 작성하고 있습니다</h3></div><strong id="augment-job-percent">0%</strong></div>
            <div id="augment-job-progress" class="wave-progress" role="progressbar" aria-label="AI 증강 진행률" aria-valuemin="0" aria-valuemax="100" aria-valuenow="0"><span></span></div>
            <p id="augment-job-message" class="job-message" aria-live="polite"></p>
            <p id="augment-error-message" class="error-message" role="alert" hidden></p>
            <div class="job-actions">
              <button id="augment-cancel-button" class="secondary-button danger ${secondary}" type="button" data-action="cancel" hidden>증강 취소</button>
            </div>
          </div>
          <div class="artifact-list">
            <div id="result-minutes-row" class="artifact-row" hidden><i class="ph ph-note-pencil" aria-hidden="true"></i><div><strong>증강 회의록</strong><code id="result-minutes"></code></div><button class="${ghost}" type="button" data-action="open-minutes" aria-label="증강 회의록 열기">열기</button></div>
          </div>
          <div class="panel-actions">
            <button id="open-minutes-button" class="primary-button ${primary}" type="button" data-action="open-minutes" hidden><i class="ph ph-file-text" aria-hidden="true"></i><span>회의록 열기</span></button>
            <button id="refine-button" class="primary-button ${primary}" type="button" data-action="refine" disabled><i class="ph ph-sparkle" aria-hidden="true"></i><span id="refine-label">AI 증강 실행</span></button>
            <span class="action-note">사전 정보 · 참석자 명부 · 단어집이 함께 적용됩니다.</span>
          </div>
        </section>
      </div>
      <footer class="app-footer"><span>Galpi ${version}</span><span>전사는 로컬에서, AI 증강은 선택한 API 제공자에서 실행됩니다.</span></footer>
    </main>

    <div id="settings-dialog" class="settings-dialog" role="dialog" aria-modal="true" aria-labelledby="settings-title" hidden>
      <div class="settings-sheet">
        <header class="settings-header">
          <div><span class="eyebrow">APP SETTINGS</span><h2 id="settings-title">설정</h2><p id="settings-message" class="settings-message" role="status" aria-live="polite" data-state="ready">변경사항은 자동으로 저장됩니다.</p></div>
          <button class="settings-close-button ${iconOutline}" type="button" data-action="close-settings" aria-label="설정 닫기"><i class="ph ph-x" aria-hidden="true"></i></button>
        </header>
        <div class="settings-body">
        <section class="settings-section" aria-labelledby="engine-settings-title">
          <div class="settings-section-heading">
            <div><strong id="engine-settings-title">전사 엔진</strong><span id="engine-settings-state">Qwen3</span></div>
            <p>다음 전사부터 적용됩니다. 준비되지 않은 엔진을 고르면 닫힌 준비 패널에서 로컬 엔진 준비를 먼저 실행해야 합니다.</p>
          </div>
          <div class="segmented-control engine-segmented" role="radiogroup" aria-label="전사 엔진 선택">
            <label data-engine-option="qwen3"><input type="radio" name="engine-preset" value="qwen3" checked /><span>Qwen3<em id="engine-qwen3-state">기본 · 확인 중</em></span></label>
            <label data-engine-option="whisperx"><input type="radio" name="engine-preset" value="whisperx" /><span>WhisperX<em id="engine-whisperx-state">이전 엔진 · 확인 중</em></span></label>
          </div>
          <div id="engine-device" hidden>
            <div class="segmented-control engine-segmented" role="radiogroup" aria-label="연산 장치 선택" aria-describedby="engine-device-help">
              <label><input type="radio" name="compute-device" value="cpu" checked /><span>CPU</span></label>
              <label><input type="radio" name="compute-device" value="cuda" /><span>NVIDIA GPU (CUDA)<em id="engine-device-cuda-note"></em></span></label>
            </div>
            <p id="engine-device-help">CUDA를 처음 선택하면 약 3.5 GB의 PyTorch CUDA 빌드를 내려받으며, 바꾼 뒤에는 로컬 엔진 준비를 다시 실행해야 합니다.</p>
          </div>
        </section>
        <section class="settings-section" aria-labelledby="token-settings-title">
          <div class="settings-section-heading">
            <div><strong id="token-settings-title">Hugging Face 토큰</strong><span>선택</span></div>
            <p>화자분리 모델을 처음 내려받을 때 사용하는 Read 전용 토큰입니다.</p>
          </div>
          <label class="sr-only" for="settings-hf-token">Hugging Face 토큰</label>
          <div class="secret-field">
            <input id="settings-hf-token" type="text" autocomplete="off" autocapitalize="none" spellcheck="false" placeholder="hf_..." aria-describedby="settings-token-help" data-visible="false" readonly />
            <button id="toggle-token-visibility" class="secret-visibility-button ${iconOutline}" type="button" data-action="toggle-token-visibility" aria-label="Hugging Face 토큰 표시"><i class="ph ph-eye" aria-hidden="true"></i></button>
          </div>
          <p id="settings-token-help">저장한 값은 이 컴퓨터의 Galpi 앱 설정에 유지되며 모델 준비 때 자동으로 사용됩니다.</p>
          <div class="token-guide-anchor">
            <button id="token-guide-trigger" class="token-guide-trigger ${ghost}" type="button" aria-expanded="false" aria-controls="token-guide-popover">필요 권한과 발급 방법 <i class="ph ph-info" aria-hidden="true"></i></button>
            <div id="token-guide-popover" class="token-guide-popover" role="dialog" aria-label="Hugging Face 토큰 발급 안내" hidden>
              <div class="token-guide-header"><strong>토큰 발급 안내</strong><button id="token-guide-close" class="${dismiss}" type="button" aria-label="닫기"><i class="ph ph-x" aria-hidden="true"></i></button></div>
              <p><strong>권장 권한:</strong> Fine-grained 토큰의 읽기(Read) 전용 권한만 사용하세요. 쓰기·추론 API 권한은 필요하지 않습니다.</p>
              <ol>
                <li>Hugging Face 계정에 로그인합니다.</li>
                <li>아래 모델 페이지에서 이용 조건에 동의하고 접근 승인을 받습니다.</li>
                <li>Settings → Access Tokens → Create new token에서 <strong>Fine-grained</strong>를 선택합니다.</li>
                <li><code>pyannote/speaker-diarization-community-1</code> 저장소 콘텐츠의 Read 권한만 허용합니다.</li>
                <li><code>hf_</code>로 시작하는 토큰을 복사해 저장합니다.</li>
              </ol>
              <p>접근 승인이 끝났거나 모델이 이미 이 컴퓨터에 준비되어 있으면 토큰을 비워 두어도 됩니다.</p>
            </div>
          </div>
          <button class="text-button ${ghost}" type="button" data-action="model-access">모델 이용 조건 페이지 열기 <i class="ph ph-arrow-up-right" aria-hidden="true"></i></button>
        </section>
        <section class="settings-section" aria-labelledby="participants-settings-title">
          <div class="settings-section-heading">
            <div><strong id="participants-settings-title">참석자 명부</strong><span id="participants-count-state">비어 있음</span></div>
            <p>한 번 만들어 두면 회의마다 참석자를 골라 화자 이름을 맞출 수 있습니다.</p>
          </div>
          <div id="participant-rows" class="participant-rows"></div>
          <p id="participant-rows-empty" class="participant-empty">아직 등록한 참석자가 없습니다.</p>
          <button class="text-button ${ghost}" type="button" data-action="add-participant"><i class="ph ph-plus" aria-hidden="true"></i> 참석자 추가</button>
        </section>
        <section class="settings-section" aria-labelledby="glossary-settings-title">
          <div class="settings-section-heading">
            <div><strong id="glossary-settings-title">단어집</strong><span id="glossary-count-state">비어 있음</span></div>
            <p>회의에서 자주 쓰는 용어를 등록하면 회의록에서 표기를 그대로 따르고 오인식을 보정합니다.</p>
          </div>
          <div id="glossary-rows" class="glossary-rows"></div>
          <p id="glossary-rows-empty" class="participant-empty">아직 등록한 용어가 없습니다.</p>
          <button class="text-button ${ghost}" type="button" data-action="add-glossary-entry"><i class="ph ph-plus" aria-hidden="true"></i> 용어 추가</button>
        </section>
        <section class="settings-section" aria-labelledby="assistant-settings-title">
          <div class="settings-section-heading">
            <div><strong id="assistant-settings-title">AI 증강</strong><span id="assistant-configured-state">API 키 없음</span><span id="chatgpt-account-state" data-state="pending" hidden>로그인 안 됨</span></div>
            <p>API 키 또는 ChatGPT 로그인으로 전사본을 회의록으로 가공합니다. 이 단계에서만 전사본이 증강 제공자로 전송됩니다.</p>
          </div>
          <div class="segmented-control engine-segmented assistant-auth-segmented" role="radiogroup" aria-label="AI 증강 인증 방식">
            <label><input type="radio" name="assistant-auth-mode" value="apiKey" checked /><span>API 키</span></label>
            <label><input type="radio" name="assistant-auth-mode" value="chatGpt" /><span>ChatGPT</span></label>
          </div>
          <div id="assistant-api-key-panel">
          <label class="sr-only" for="settings-assistant-key">AI 증강 API 키</label>
          <div class="secret-field">
            <input id="settings-assistant-key" type="text" autocomplete="off" autocapitalize="none" spellcheck="false" placeholder="API 키" aria-describedby="settings-assistant-help" data-visible="false" readonly />
            <button id="toggle-assistant-visibility" class="secret-visibility-button ${iconOutline}" type="button" data-action="toggle-assistant-visibility" aria-label="API 키 표시"><i class="ph ph-eye" aria-hidden="true"></i></button>
          </div>
          <p id="settings-assistant-help">사용 중인 OpenAI 호환 서비스(z.ai 코딩 플랜, OpenRouter 등)에서 발급한 API 키를 사용합니다.</p>
          <label class="settings-field-label" for="settings-assistant-model">가공 모델</label>
          <input id="settings-assistant-model" class="settings-input" type="text" list="assistant-model-suggestions" autocomplete="off" autocapitalize="none" spellcheck="false" placeholder="glm-5.3-flash" aria-describedby="settings-model-help" />
          <datalist id="assistant-model-suggestions">
            <option value="glm-5.3-flash">z.ai · 기본값</option>
            <option value="glm-5.3">z.ai · 플래그십</option>
            <option value="glm-5.2">z.ai · 이전 플래그십</option>
            <option value="glm-5-turbo">z.ai · 빠른 응답</option>
            <option value="glm-4.6">z.ai · 이전 세대</option>
          </datalist>
          <p id="settings-model-help">z.ai 코딩 플랜의 GLM 모델이 기본값입니다. 다른 제공자를 쓸 때는 그 제공자의 모델 이름을 그대로 입력하세요. 긴 회의는 상위 모델이, 짧은 회의는 기본값인 Flash가 유리합니다.</p>
          <label class="settings-field-label" for="settings-assistant-effort">추론 강도</label>
          <select id="settings-assistant-effort" class="settings-select" aria-describedby="settings-effort-help">
            <option value="">제공자 기본값</option>
            <option value="low">낮음</option>
            <option value="medium">중간</option>
            <option value="high">높음</option>
            <option value="max" selected>최대</option>
          </select>
          <p id="settings-effort-help">모델이 회의록을 작성하기 전에 숙고하는 깊이입니다. GLM 모델은 최대가 기본이며, 추론 과정도 진행 상황에 표시됩니다.</p>
          <label class="settings-field-label" for="settings-assistant-base-url">API 주소 (선택)</label>
          <input id="settings-assistant-base-url" class="settings-input" type="text" autocomplete="off" autocapitalize="none" spellcheck="false" placeholder="https://api.z.ai/api/coding/paas/v4 (기본값)" aria-describedby="settings-base-url-help" />
          <p id="settings-base-url-help">OpenAI 호환 엔드포인트라면 모두 사용할 수 있습니다. OpenRouter는 https://openrouter.ai/api/v1 를 입력하세요. 비워 두면 z.ai 코딩 플랜 주소를 사용합니다.</p>
          </div>
          <div id="assistant-chatgpt-panel" hidden>
            <p id="chatgpt-status" class="chatgpt-status" data-state="signedOut">ChatGPT에 로그인하지 않았습니다.</p>
            <div class="chatgpt-actions">
              <button id="chatgpt-sign-in-button" class="primary-button ${primary}" type="button" data-action="sign-in-chatgpt">ChatGPT로 계속하기</button>
              <button id="chatgpt-cancel-button" class="secondary-button ${secondary}" type="button" data-action="cancel-chatgpt-sign-in" hidden>취소</button>
              <button id="chatgpt-sign-out-button" class="secondary-button danger ${secondary}" type="button" data-action="sign-out-chatgpt" hidden>ChatGPT 로그아웃</button>
            </div>
            <p id="chatgpt-message" class="settings-message" role="status" aria-live="polite" data-state="ready"></p>
            <div id="chatgpt-welcome" class="augment-hint" hidden>
              <i class="ph ph-info" aria-hidden="true"></i>
              <div>
                <p>ChatGPT 요금제로 회의록을 정제합니다. 사용량은 ChatGPT 요금제의 한도에서 차감되며, 한도는 다른 앱과 함께 씁니다.</p>
                <button class="text-button ${ghost}" type="button" data-action="acknowledge-chatgpt-welcome">확인</button>
              </div>
            </div>
            <div id="chatgpt-model-field" hidden>
              <label class="settings-field-label" for="settings-chatgpt-model">가공 모델</label>
              <select id="settings-chatgpt-model" class="settings-select" aria-describedby="chatgpt-plan-line"></select>
              <p id="chatgpt-plan-line">ChatGPT 요금제 사용 중 · <button class="text-button ${ghost}" type="button" data-action="open-chatgpt-usage">사용량 관리</button></p>
            </div>
            <p id="chatgpt-data-notice">회의록 정제 때 전사문·참석자·단어집·사전 정보가 OpenAI API로 전송됩니다. ChatGPT 대화·계정 정보에는 접근하지 않습니다.</p>
          </div>
          <label class="settings-field-label" for="settings-assistant-background">사전 정보</label>
          <textarea id="settings-assistant-background" class="settings-textarea" rows="8" autocomplete="off" autocapitalize="none" spellcheck="false" aria-describedby="settings-background-help" placeholder="제품/서비스: 갈피 (회의 녹음·전사 데스크톱 앱)&#10;팀: 하빈(팀리더), 지우(백엔드)&#10;별칭: 프로님 = 하빈&#10;도메인 용어: 화자분리, 정렬 체크포인트"></textarea>
          <p id="settings-background-help">참석자·제품명·약어·도메인 용어를 적어 두면 잘못 들린 단어와 화자를 보정합니다. 이 컴퓨터에만 저장되고 회의록을 만들 때 함께 전송됩니다.</p>
        </section>
        <div class="settings-actions">
          <button class="secondary-button danger ${secondary}" type="button" data-action="clear-token">Hugging Face 토큰 지우기</button>
          <button class="secondary-button danger ${secondary}" type="button" data-action="clear-assistant-key">AI 증강 API 키 지우기</button>
        </div>
        </div>
      </div>
    </div>
  </div>
`
