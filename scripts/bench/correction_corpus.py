"""Gold Korean meetings and a seeded ASR/diarization noise model.

Each meeting is written clean, with the roster, glossary, and background the
user would have saved. `generate` turns it into what Galpi's worker actually
hands the refinement step: a `_화자별.txt` transcript with anonymous speaker
labels, short timestamped segments, glossary/name misrecognitions, and
speaker-boundary errors where the tail of one person's sentence is assigned
to the next speaker. Every injected error is recorded as gold.

Errors come from per-term variant lists that mimic real Korean ASR output
(loanword spelling drift, spacing splits, near-homophones). `distractors`
are correct words that look like a variant; hinting them is a false positive.
"""

import random
import re
from dataclasses import dataclass, field

SEED = "galpi-correction-v1"
CORRUPT_PROBABILITY = 0.65
# Jargon outside the glossary drifts more: nothing anchors its spelling.
UNLISTED_CORRUPT_PROBABILITY = 0.8
BOUNDARY_ERRORS_PER_MEETING = 2
# A short reply right after a question is often stamped with the asker's
# label at the same second, or left as UNKNOWN when no cluster claims it.
BACKCHANNEL_PROBABILITY = 0.5
UNKNOWN_SHARE = 0.35
BACKCHANNELS = ("네.", "맞습니다.", "그렇죠.", "네, 맞아요.")
SEGMENT_MAX_CHARS = 42
CHARS_PER_SECOND = 7.0
HONORIFIC = "님"


@dataclass(frozen=True)
class Person:
    key: str
    name: str
    aliases: tuple[str, ...] = ()
    team: str | None = None
    role: str | None = None
    description: str | None = None
    on_roster: bool = True


@dataclass(frozen=True)
class Meeting:
    id: str
    title: str
    background: str
    people: tuple[Person, ...]
    glossary: tuple[tuple[str, str | None], ...]
    # surface written correctly in the turns -> ASR variants of it
    term_variants: dict[str, tuple[str, ...]]
    background_variants: dict[str, tuple[str, ...]]
    name_variants: dict[str, tuple[str, ...]]
    distractors: tuple[str, ...]
    turns: tuple[tuple[str, str], ...]
    # jargon in neither glossary nor background -> ASR variants of it
    unlisted_variants: dict[str, tuple[str, ...]] = field(default_factory=dict)


@dataclass(frozen=True)
class LexicalError:
    surface: str
    correct: str
    kind: str  # "term" | "background" | "name" | "unlisted"


@dataclass(frozen=True)
class BoundaryError:
    second: int
    shown_label: str
    true_label: str


@dataclass
class Generated:
    meeting: Meeting
    transcript: str
    labels: dict[str, str]  # person key -> SPEAKER_xx
    speaker_names: dict[str, str | None]  # SPEAKER_xx -> roster name or None
    lexical_errors: list[LexicalError] = field(default_factory=list)
    boundary_errors: list[BoundaryError] = field(default_factory=list)
    line_count: int = 0
    # unlisted term -> every surface it took in the transcript
    clusters: dict[str, set[str]] = field(default_factory=dict)

    def participants_payload(self) -> list[dict[str, object]]:
        return [
            {
                "name": person.name,
                "team": person.team,
                "role": person.role,
                "description": person.description,
                "aliases": list(person.aliases),
            }
            for person in self.meeting.people
            if person.on_roster
        ]

    def glossary_payload(self) -> list[dict[str, object]]:
        return [
            {"term": term, "description": description}
            for term, description in self.meeting.glossary
        ]


MEETINGS: tuple[Meeting, ...] = (
    Meeting(
        id="galpi-quality",
        title="갈피 전사 품질 점검",
        background="갈피는 회의를 녹음하고 로컬에서 전사하는 데스크톱 앱이다. 맥은 Qwen3, 윈도우는 WhisperX로 전사한다.",
        people=(
            Person(
                "hb", "하빈", ("빈",), "갈피팀", "팀리더", "일정과 우선순위를 정한다"
            ),
            Person("jw", "지우", (), "갈피팀", "백엔드", "워커와 전사 파이프라인 담당"),
            Person("sy", "서연", (), "갈피팀", "디자인", "설정 화면과 온보딩 담당"),
        ),
        glossary=(
            ("Qwen3", "맥 기본 전사 엔진"),
            ("WhisperX", "이전 전사 엔진"),
            ("pyannote", "화자분리 모델"),
            ("화자분리", None),
            ("Tauri", "데스크톱 앱 프레임워크"),
            ("갈피", "제품 이름"),
        ),
        term_variants={
            "Qwen3": ("큐원 쓰리", "큐엔3", "퀜 3"),
            "WhisperX": ("위스퍼 엑스", "위스퍼엑스", "휘스퍼X"),
            "pyannote": ("파이어노트", "파이 아노트"),
            "화자분리": ("화자 불리", "하자분리"),
            "Tauri": ("타우리", "타오리"),
            "갈피": ("칼피", "갈 피"),
        },
        background_variants={},
        name_variants={
            "하빈": ("하민", "화빈"),
            "지우": ("지후", "지유"),
            "서연": ("서현",),
        },
        distractors=("갈비", "불리한"),
        turns=(
            (
                "hb",
                "자, 시작할게요. 오늘은 갈피 전사 품질 점검이고요, 저 하빈이 진행하겠습니다.",
            ),
            ("hb", "지우 님, 지난주에 Qwen3로 돌린 결과부터 공유해 주시겠어요?"),
            (
                "jw",
                "네, 열 개 회의 중에 여덟 개는 Qwen3가 WhisperX보다 오류율이 낮았어요. 나머지 두 개는 잡음이 심한 녹음이었고요.",
            ),
            ("hb", "잡음이 심한 건 어떤 환경이었어요?"),
            (
                "jw",
                "카페에서 노트북 마이크로 녹음한 거였어요. 그런 경우엔 pyannote 화자분리도 같이 흔들렸습니다.",
            ),
            (
                "sy",
                "디자인 쪽에서 보면, 녹음 환경이 나쁠 때 미리 경고를 띄우는 게 좋겠어요.",
            ),
            ("hb", "서연 님 생각엔 경고를 어디에 띄우면 좋을까요?"),
            (
                "sy",
                "녹음 시작 화면에 입력 레벨 표시를 두고, 잡음이 기준을 넘으면 문구를 보여 주면 돼요. 시안은 목요일까지 드릴게요.",
            ),
            ("jw", "그 기준값은 제가 워커 쪽에서 계산해서 넘겨 드릴게요."),
            ("hb", "좋아요. 그럼 경고 표시는 이번 스프린트에 넣는 걸로 정하죠."),
            (
                "jw",
                "하나 더 있는데요, Tauri 업데이트하면서 윈도우에서 화자분리 모델 다운로드가 실패하는 문제가 있었어요.",
            ),
            ("hb", "원인은 찾았어요?"),
            (
                "jw",
                "토큰을 안 넘기고 있었어요. 지금은 고쳐서 윈도우에서도 pyannote가 정상으로 받아집니다.",
            ),
            (
                "sy",
                "그거 사용자한테는 어떻게 보였어요? 에러 문구가 너무 기술적이면 불리한 인상을 줄 수 있거든요.",
            ),
            (
                "jw",
                "영어 원문 에러가 그대로 나왔어요. 문구 정리는 서연 님이 같이 봐 주시면 좋겠어요.",
            ),
            ("sy", "네, 오류 문구 목록 주시면 한국어로 다듬을게요."),
            ("hb", "WhisperX는 언제까지 남겨 둘 거예요?"),
            (
                "jw",
                "윈도우가 아직 Qwen3를 못 쓰니까 당분간은 남겨야 해요. 맥에서만 기본값을 Qwen3로 두고 있습니다.",
            ),
            ("hb", "알겠어요. 그럼 WhisperX 제거는 보류하고, 다음 분기에 다시 보죠."),
            ("sy", "회의 끝나고 다 같이 갈비 먹으러 가요. 제가 예약해 둘게요."),
            (
                "hb",
                "좋습니다. 정리하면 서연 님은 목요일까지 경고 시안, 지우 님은 기준값 계산이에요.",
            ),
            ("jw", "네, 기준값은 다음 주 화요일까지 공유하겠습니다."),
        ),
    ),
    Meeting(
        id="payment-release",
        title="결제 시스템 릴리스 점검",
        background="결제팀은 PG사 웹훅 포맷 변경에 맞춰 결제 API를 배포한다. 정산 배치는 매일 새벽 두 시에 돈다.",
        people=(
            Person("mj", "민준", (), "결제팀", "팀장", "릴리스 승인과 일정 결정"),
            Person("sa", "수아", (), "결제팀", "백엔드", "정산 배치와 웹훅 담당"),
            Person("dy", "도윤", (), "결제팀", "QA", "회귀 테스트 담당"),
            Person("g1", "PG사 담당자", on_roster=False),
        ),
        glossary=(
            ("결제", None),
            ("정산 배치", "매일 새벽 정산 작업"),
            ("웹훅", None),
            ("멱등키", "중복 요청 방지 키"),
            ("카나리 배포", None),
            ("롤백", None),
        ),
        term_variants={
            "정산 배치": ("정산 패치", "정상 배치"),
            "웹훅": ("웹 훅", "웹국", "웹호크"),
            "멱등키": ("멱등 키", "면등키", "명등키"),
            "카나리 배포": ("카나리아 배포", "까나리 배포"),
            "롤백": ("롤 백", "로백"),
        },
        background_variants={},
        name_variants={
            "민준": ("민중", "민정"),
            "수아": ("수하", "수야"),
            "도윤": ("도연",),
        },
        distractors=("결재", "정상으로"),
        turns=(
            (
                "mj",
                "결제팀 릴리스 점검 시작하겠습니다. 오늘은 PG사 담당자분도 함께 들어오셨어요.",
            ),
            (
                "g1",
                "안녕하세요, PG사 연동 담당입니다. 이번 웹훅 포맷 변경 때문에 참석했습니다.",
            ),
            ("mj", "수아 님, 웹훅 변경 반영 상황 먼저 말씀해 주세요."),
            (
                "sa",
                "새 포맷 파싱은 다 됐고요, 중복 수신에 대비해서 멱등키 검사도 넣었습니다.",
            ),
            ("g1", "재전송은 최대 세 번까지 하니까 멱등키 검사는 꼭 필요합니다."),
            (
                "dy",
                "QA 쪽에서는 중복 수신 시나리오를 아직 못 돌렸어요. 테스트 환경에서 재전송을 흉내 낼 방법이 없어서요.",
            ),
            (
                "sa",
                "제가 재전송을 흉내 내는 스크립트를 만들어 드릴게요. 내일 오전까지 가능합니다.",
            ),
            ("mj", "도윤 님, 그 스크립트 받으면 회귀 테스트는 언제 끝나요?"),
            (
                "dy",
                "네, 민준 님. 금요일까지는 끝낼 수 있어요. 결제 취소 케이스까지 포함해서요.",
            ),
            ("mj", "그럼 배포 방식은 어떻게 할까요? 전체 배포는 부담스럽거든요."),
            (
                "sa",
                "카나리 배포로 오 퍼센트부터 열고, 에러율 보면서 늘리는 게 안전합니다.",
            ),
            ("mj", "좋아요, 카나리 배포로 가죠. 이상이 있으면 바로 롤백하고요."),
            (
                "dy",
                "롤백 기준은 숫자로 정해 두는 게 좋겠습니다. 결제 실패율이 평소의 두 배를 넘으면 롤백하는 걸로요.",
            ),
            (
                "mj",
                "그 기준으로 하죠. 그리고 배포 전에 본부장님 결재가 필요하니까 제가 오늘 올려 둘게요.",
            ),
            (
                "sa",
                "정산 배치는 이번에 안 건드리니까 영향이 없습니다. 다만 배치가 새벽 두 시에 돌아서 배포는 그 전에 끝내야 해요.",
            ),
            ("g1", "저희 쪽 웹훅 전환은 다음 주 월요일 오전 열 시에 합니다."),
            (
                "mj",
                "그럼 우리 배포는 그 전 금요일 오후로 잡겠습니다. 수아 님이 배포 담당해 주세요.",
            ),
            ("sa", "네, 제가 맡을게요. 정산 배치 모니터링도 같이 보겠습니다."),
            (
                "dy",
                "중간 결과는 목요일 저녁에 한 번 공유드릴게요. 정상으로 나오면 그대로 진행하시면 됩니다.",
            ),
            (
                "mj",
                "정리하면 금요일 오후 카나리 배포, 롤백 기준은 실패율 두 배, 배포 담당은 수아 님입니다.",
            ),
        ),
    ),
    Meeting(
        id="moa-retention",
        title="모아 앱 리텐션 캠페인",
        background="모아는 우리 회사의 가계부 앱이다. 이번 분기 목표는 30일 리텐션을 25%에서 30%로 올리는 것이다. 인앱 메시지는 브레이즈로 발송한다.",
        people=(
            Person("yr", "예린", (), "그로스팀", "그로스 리드", "캠페인 우선순위 결정"),
            Person(
                "hw",
                "현우",
                ("현우 매니저",),
                "그로스팀",
                "마케팅",
                "메시지 문구와 발송",
            ),
            Person("jm", "지민", (), "데이터팀", "데이터 분석", "코호트와 퍼널 분석"),
        ),
        glossary=(
            ("리텐션", None),
            ("퍼널", None),
            ("인앱 메시지", None),
            ("코호트", None),
            ("CAC", "고객 획득 비용"),
        ),
        term_variants={
            "리텐션": ("리텐선", "리텐숀"),
            "퍼널": ("퍼넬", "퍼놀"),
            "인앱 메시지": ("인앱 메세지", "이냅 메시지"),
            "코호트": ("코호드", "코 호트"),
            "CAC": ("캑", "씨에이씨"),
        },
        background_variants={
            "모아": ("모하", "머아"),
            "브레이즈": ("브레이스", "블레이즈"),
        },
        name_variants={
            "예린": ("애린", "예림"),
            "현우": ("현오", "형우"),
            "지민": ("지빈",),
        },
        distractors=("모아서", "퍼센트"),
        turns=(
            (
                "yr",
                "그로스 주간 회의 시작할게요. 저는 예린이고, 오늘은 모아 리텐션 캠페인 얘기만 하겠습니다.",
            ),
            ("yr", "지민 님, 지난달 코호트 분석 결과 정리된 거 있어요?"),
            (
                "jm",
                "네, 가입 첫 주에 예산을 등록한 사용자는 30일 리텐션이 12퍼센트 높았어요.",
            ),
            (
                "hw",
                "그럼 가입 첫 주에 예산 등록을 유도하는 인앱 메시지를 보내면 되겠네요.",
            ),
            ("yr", "현우 매니저님, 메시지는 브레이즈로 바로 만들 수 있어요?"),
            ("hw", "템플릿이 있어서 이번 주 안에 세 가지 문구로 만들 수 있어요."),
            (
                "jm",
                "실험으로 가려면 대조군을 꼭 두셔야 해요. 안 그러면 효과를 분리할 수가 없어요.",
            ),
            ("yr", "맞아요. 대조군은 20퍼센트로 두죠."),
            ("hw", "CAC 얘기도 해야 하는데, 이번 달 광고 단가가 많이 올랐어요."),
            (
                "jm",
                "퍼널 기준으로 보면 설치에서 가입으로 넘어가는 비율이 떨어진 게 더 커요. 광고 단가만의 문제는 아닙니다.",
            ),
            (
                "yr",
                "그럼 가입 퍼널 이탈 구간을 지민 님이 다음 주 수요일까지 쪼개서 봐 주세요.",
            ),
            ("jm", "네, 단계별 이탈률로 정리해서 드릴게요."),
            ("hw", "의견 모아서 문구 세 개 정리한 다음 예린 님께 먼저 보여 드릴게요."),
            ("yr", "좋아요. 문구 확정은 금요일 회의에서 하죠."),
            (
                "hw",
                "모아 앱 3.2 버전에 새 온보딩 화면이 들어가는데, 그거랑 메시지가 겹치지 않을까요?",
            ),
            ("jm", "겹치면 실험 결과가 섞여요. 온보딩 배포일을 확인해야 합니다."),
            (
                "yr",
                "온보딩 배포일은 제가 제품팀에 확인할게요. 확인 전까지 메시지 발송은 보류합니다.",
            ),
            ("hw", "알겠습니다. 문구 작업은 먼저 해 둘게요."),
        ),
    ),
    Meeting(
        id="api-incident",
        title="결제 API 장애 회고",
        background="어제 오후 두 시부터 사십 분간 결제 API 응답이 느려졌다. 서비스는 쿠버네티스 위에서 돌고 DB는 PostgreSQL이다.",
        people=(
            Person("ty", "태윤", (), "SRE팀", "SRE 리드", "회고 진행"),
            Person("sh", "소희", (), "결제팀", "백엔드", "결제 API와 DB 설정"),
            Person("js", "준서", (), "SRE팀", "SRE", "모니터링과 알림"),
        ),
        glossary=(
            ("쿠버네티스", None),
            ("헬름 차트", None),
            ("오토스케일러", None),
            ("프로메테우스", "알림 시스템"),
            ("그라파나", "대시보드"),
            ("레이턴시", None),
            ("커넥션 풀", None),
        ),
        term_variants={
            "쿠버네티스": ("쿠버네티즈", "쿠베르네티스"),
            "헬름 차트": ("헬멧 차트", "헬름 자트"),
            "오토스케일러": ("오토 스케일러", "오토스캘러"),
            "프로메테우스": ("프로메테오스", "프로메테우수"),
            "그라파나": ("그라파냐", "크라파나"),
            "레이턴시": ("레이턴스", "래이턴시"),
            "커넥션 풀": ("커넥션 플", "컨넥션 풀"),
        },
        background_variants={},
        name_variants={
            "태윤": ("태연", "대윤"),
            "소희": ("소이", "서희"),
            "준서": ("준석",),
        },
        distractors=("레이어", "스케일"),
        turns=(
            (
                "ty",
                "어제 결제 API 장애 회고 시작하겠습니다. 진행은 저 태윤이 맡겠습니다.",
            ),
            ("ty", "준서 님, 타임라인부터 정리해 주시겠어요?"),
            (
                "js",
                "오후 두 시 십 분에 프로메테우스 알림이 먼저 울렸고요, 그라파나에서 레이턴시가 평소의 다섯 배까지 올라간 걸 확인했습니다.",
            ),
            (
                "js",
                "두 시 이십 분에 오토스케일러가 파드를 늘렸는데도 레이턴시가 안 내려갔어요.",
            ),
            (
                "sh",
                "그게 커넥션 풀 때문이었어요. 파드가 늘어나니까 DB 커넥션이 한도를 넘어서 오히려 더 막혔습니다.",
            ),
            ("ty", "소희 님, 커넥션 풀 설정은 어디서 관리되고 있어요?"),
            (
                "sh",
                "헬름 차트 값으로 들어가 있어요. 파드당 오십 개라서 파드가 열 개를 넘으면 DB 한도를 넘습니다.",
            ),
            ("js", "그럼 오토스케일러 최대값이랑 커넥션 풀 크기를 같이 봐야겠네요."),
            ("ty", "재발 방지 대책은 뭐로 할까요?"),
            (
                "sh",
                "파드당 커넥션을 이십 개로 줄이고, 앞단에 커넥션 프록시를 두는 걸 제안합니다.",
            ),
            (
                "js",
                "스케일 아웃 상한도 쿠버네티스 쪽에서 열두 개로 묶어 두는 게 좋겠습니다.",
            ),
            (
                "ty",
                "두 가지 다 하죠. 소희 님은 헬름 차트 수정을 다음 주 화요일까지 해 주세요.",
            ),
            ("sh", "네, 프록시 도입 검토 문서도 같이 올리겠습니다."),
            ("ty", "준서 님은 프로메테우스 알림 기준을 레이턴시 세 배로 낮춰 주세요."),
            ("js", "알겠습니다. 그라파나 대시보드에 커넥션 사용률 패널도 추가할게요."),
            ("sh", "로드밸런서 레이어 쪽은 문제없었던 거 맞죠?"),
            ("js", "네, 로드밸런서는 정상이었습니다."),
            ("ty", "그럼 회고 문서는 제가 오늘 안에 정리해서 공유하겠습니다."),
        ),
    ),
    Meeting(
        id="hiring-committee",
        title="백엔드 채용 위원회",
        background="백엔드 시니어 포지션 채용 중이다. 최종 후보자는 두 명이다.",
        people=(
            Person("ej", "은지", (), "인사팀", "채용 담당", "채용 절차와 안내 메일"),
            Person("sm", "성민", (), "백엔드팀", "팀장", "채용 최종 결정"),
            Person(
                "hj", "한결", (), "백엔드팀", "시니어 엔지니어", "면접관, 입사자 온보딩"
            ),
        ),
        glossary=(
            ("컬처핏", None),
            ("레퍼런스 체크", None),
            ("처우 협의", None),
            ("온보딩", None),
            ("코딩 테스트", None),
        ),
        term_variants={
            "컬처핏": ("컬쳐핏", "컬처 핏", "컬처빗"),
            "레퍼런스 체크": ("레퍼런스 책", "레퍼런스 체그"),
            "처우 협의": ("처우 협회", "처유 협의"),
            "온보딩": ("온 보딩", "언보딩"),
            "코딩 테스트": ("코딩 테스크", "코딩 테수트"),
        },
        background_variants={},
        name_variants={
            "은지": ("은주", "은치"),
            "성민": ("승민", "성빈"),
            "한결": ("한겨",),
        },
        distractors=("체크인",),
        turns=(
            (
                "ej",
                "채용 위원회 시작할게요. 채용 담당 은지입니다. 오늘은 백엔드 후보자 두 분을 결정해야 해요.",
            ),
            ("ej", "성민 님, 첫 번째 후보자 면접 어떠셨어요?"),
            (
                "sm",
                "코딩 테스트 점수는 높았는데, 설계 면접에서 트레이드오프 설명이 약했어요.",
            ),
            (
                "hj",
                "저는 오히려 장애 대응 경험이 좋았다고 봤어요. 실무에서는 그게 더 중요하거든요.",
            ),
            ("sm", "컬처핏 쪽은 어떻게 보셨어요?"),
            (
                "hj",
                "코드 리뷰 피드백을 받아들이는 태도가 좋았습니다. 컬처핏은 문제없다고 봐요.",
            ),
            (
                "ej",
                "그럼 첫 번째 후보자는 레퍼런스 체크를 진행하고 결과를 보고 결정하는 걸로 할까요?",
            ),
            (
                "sm",
                "네, 그렇게 하죠. 레퍼런스 체크는 전 직장 팀장 한 분이면 충분합니다.",
            ),
            ("ej", "두 번째 후보자는요?"),
            (
                "hj",
                "두 번째 분은 경력은 길지만 최근 삼 년은 관리 업무만 하셨어요. 코딩 테스트도 시간 안에 못 끝냈고요.",
            ),
            ("sm", "저도 같은 의견입니다. 이번 포지션에는 맞지 않아요."),
            (
                "ej",
                "그럼 두 번째 후보자는 불합격으로 정리하겠습니다. 안내 메일은 제가 내일 보낼게요.",
            ),
            ("sm", "첫 번째 후보자 처우 협의는 언제부터 가능해요?"),
            (
                "ej",
                "레퍼런스 체크가 다음 주 수요일에 끝나니까, 처우 협의는 그 뒤에 시작할게요.",
            ),
            (
                "hj",
                "입사하게 되면 온보딩은 제가 맡을게요. 첫날 체크인 일정도 제가 잡겠습니다.",
            ),
            ("sm", "좋습니다. 온보딩 자료는 지난번 걸 업데이트해서 쓰시면 돼요."),
            (
                "ej",
                "정리하면 첫 번째 후보자는 레퍼런스 체크 후 처우 협의, 두 번째는 불합격입니다.",
            ),
        ),
    ),
    Meeting(
        id="clinical-data",
        title="임상 연구 데이터 협의",
        background="대학병원과 함께 진행하는 당뇨 합병증 예측 연구다. 병원에서 가명처리된 진료 데이터를 받는다.",
        people=(
            Person(
                "ps",
                "박서준",
                ("박 교수님", "교수님"),
                "의과대학",
                "책임연구자",
                "연구 설계와 승인",
            ),
            Person(
                "lh",
                "이하은",
                ("하은 연구원",),
                "의과대학",
                "연구원",
                "IRB와 동의서 담당",
            ),
            Person(
                "kd",
                "김도현",
                ("도현",),
                "데이터팀",
                "데이터 엔지니어",
                "데이터 적재와 분석 환경",
            ),
        ),
        glossary=(
            ("IRB", "기관생명윤리위원회"),
            ("코호트", None),
            ("가명처리", None),
            ("결측치", None),
            ("데이터 레이크", None),
        ),
        term_variants={
            "IRB": ("아이알비", "아이 알 비"),
            "코호트": ("코호드",),
            "가명처리": ("가면처리", "가명처래"),
            "결측치": ("결척치", "결측 치"),
            "데이터 레이크": ("데이터 레이커", "데이타 레이크"),
        },
        background_variants={},
        name_variants={
            "박서준": ("박서중",),
            "이하은": ("이하연",),
            "하은": ("하연",),
            "김도현": ("김도연",),
            "도현": ("도연",),
        },
        distractors=("이하는",),
        turns=(
            ("ps", "연구 데이터 협의 시작하겠습니다. 책임연구자 박서준입니다."),
            ("ps", "하은 연구원님, IRB 승인 상황 먼저 알려 주세요."),
            (
                "lh",
                "IRB는 지난주에 승인됐고요, 동의서 양식만 한 군데 수정 요청이 왔습니다.",
            ),
            (
                "kd",
                "데이터를 받으려면 가명처리가 먼저 끝나야 하는데, 병원 쪽 일정은 정해졌나요?",
            ),
            ("lh", "병원에서는 다음 달 첫 주에 가명처리된 파일을 준다고 했어요."),
            ("ps", "도현 씨, 받은 데이터는 어디에 올려요?"),
            (
                "kd",
                "데이터 레이크에 연구 전용 영역을 따로 만들어서 올리겠습니다. 접근 권한은 세 분만 열어 둘게요.",
            ),
            (
                "lh",
                "코호트 정의는 어떻게 할까요? 진단 후 6개월 추적이 안 된 환자도 있어요.",
            ),
            ("ps", "추적 기간이 6개월 미만인 환자는 코호트에서 제외합니다."),
            (
                "kd",
                "결측치 기준도 정해야 합니다. 검사 수치가 빠진 비율이 높은 변수가 꽤 있어요.",
            ),
            (
                "ps",
                "결측치 비율이 삼십 퍼센트 이하는 대체하고, 그 이상은 변수에서 빼죠.",
            ),
            ("lh", "그럼 결측치 현황표를 제가 데이터 받는 대로 만들게요."),
            (
                "kd",
                "분석 코드는 데이터 레이크 안에서만 돌게 설정하겠습니다. 외부 반출은 막아 둘게요.",
            ),
            (
                "ps",
                "좋습니다. 동의서 수정은 하은 연구원님이 이번 주 금요일까지 IRB에 다시 제출해 주세요.",
            ),
            ("lh", "네, 교수님. 금요일 오전에 제출하겠습니다."),
            ("ps", "다음 회의는 데이터 받은 뒤에 하겠습니다."),
        ),
    ),
    # Error patterns from a real Galpi transcript of a backend handover (no
    # content reused): loanword jargon spelled several ways in one meeting,
    # names heard as common words (승민님 -> 승인님), replies stamped with the
    # asker's label, and jargon that no glossary lists.
    Meeting(
        id="reward-handover",
        title="리워드 요율 정책 인수인계",
        background="리워드는 사용자 매출의 일정 비율을 돌려주는 기능이다. 이번 개편으로 고정 20% 대신 정책별 요율을 적용한다.",
        people=(
            Person(
                "sm",
                "승민",
                (),
                "플랫폼팀",
                "백엔드",
                "리워드 정산 개발, 이번 주까지 근무",
            ),
            Person("hb", "하빈", (), "플랫폼팀", "테크 리드", "설계 검토와 배포 결정"),
            Person("dh", "도현", (), "플랫폼팀", "백엔드", "리워드 기능 인수자"),
        ),
        glossary=(
            ("요율", None),
            ("실효율", "실제 적용되는 리워드 비율"),
            ("폴리시", "요율 정책 레코드"),
            ("디스트리뷰션", "메시지 매출 분배 레코드"),
            ("마이그레이션", None),
            ("페르소나", None),
        ),
        term_variants={
            "요율": ("유율", "요요"),
            "실효율": ("실요율", "실요일", "시효율"),
            "폴리시": ("팔러시", "팔레시", "폴리쉬"),
            "디스트리뷰션": ("디스트리비션", "디스트리뷰숀"),
            "마이그레이션": ("마이브레이션", "마이그레이선"),
            "페르소나": ("페리소나", "펠르소나"),
        },
        background_variants={},
        name_variants={
            "승민": ("승인", "승리", "생민"),
            "하빈": ("하비", "합의"),
            "도현": ("도연",),
        },
        distractors=("승인", "합의된"),
        unlisted_variants={
            "레베뉴": ("레벤유", "레벤뉴", "레베 뉴"),
            "트랜잭션": ("트랜젝션", "트랜직션"),
            "어드민": ("어드밍", "어드밀"),
        },
        turns=(
            (
                "hb",
                "인수인계 시작할게요. 승민님이 이번 주까지라서 리워드 쪽은 오늘 다 정리하려고요.",
            ),
            ("hb", "승민님, 이번에 바뀐 계산 방식부터 설명해 주세요."),
            (
                "sm",
                "예전에는 요율이 20% 고정이었는데, 이제 폴리시마다 요율을 따로 둡니다. 메시지 트랜잭션 잡이 돌면서 디스트리뷰션을 만들고, 그때 폴리시의 실효율을 곱해요.",
            ),
            ("dh", "그럼 레베뉴 레코드에도 실효율이 같이 저장되나요?"),
            (
                "sm",
                "네, 도현님. 레베뉴 테이블에 목표치랑 실효율을 둘 다 컬럼으로 남겨 둡니다. 나중에 폴리시가 바뀌어도 그때 값이 남아요.",
            ),
            ("hb", "요율 표가 비어 있으면 어떻게 돼요?"),
            (
                "sm",
                "그때는 커서가 멈춰서 지급이 아예 멈춥니다. 에러로 끝나는 것보다 그게 안전하다고 봤어요.",
            ),
            ("dh", "멈춘 동안 쌓인 건 요율을 넣으면 한꺼번에 처리되나요?"),
            (
                "sm",
                "네, 커서가 멈춘 지점부터 다시 돌아서 밀린 디스트리뷰션이 한 번에 만들어집니다.",
            ),
            ("hb", "배포할 때 마이그레이션에 기본 폴리시를 넣어야겠네요."),
            (
                "sm",
                "맞아요. 마이그레이션으로 기본 4% 폴리시를 넣고, 처음에는 꺼 둔 상태로 배포하려고요.",
            ),
            (
                "hb",
                "그건 본부 승인 받고 진행하죠. 합의된 일정대로 다음 주 월요일 배포입니다.",
            ),
            ("dh", "페르소나별 요율은 어드민에서 등록하는 거죠?"),
            (
                "sm",
                "네, 어드민 리워드 요율 관리 탭에서 페르소나별이나 사용자별로 등록하면 폴리시가 하나씩 늘어나요.",
            ),
            (
                "hb",
                "폴리시 삭제는 막는 게 좋겠어요. 레베뉴가 참조하고 있으면 조인했을 때 비어 버리니까요.",
            ),
            ("sm", "그럼 삭제 버튼은 빼고 사용 여부만 끌 수 있게 바꾸겠습니다."),
            (
                "dh",
                "실효율 표는 기본키를 따로 두는 게 낫지 않을까요? 지금은 요율 값 자체가 키라서 나중에 바꾸기 어려워요.",
            ),
            ("hb", "도현님 말대로 자동 증가 키를 두고 요율 조합은 유니크로 걸죠."),
            (
                "sm",
                "네, 그렇게 바꾸고 기본 폴리시는 항상 하나만 켜지도록 로직에서 막겠습니다.",
            ),
            (
                "hb",
                "정리하면 승민님은 키 변경이랑 삭제 막기를 금요일까지 하고, 도현님은 월요일 배포를 맡아 주세요.",
            ),
            ("dh", "네, 월요일 배포 전에 어드민 화면도 한 번 더 확인하겠습니다."),
        ),
    ),
)

SENTENCE_END = re.compile(r"(?<=[.?!])\s+")
SEEDS = ("galpi-correction-v1", "galpi-correction-v2", "galpi-correction-v3")
PROCEDURAL_SHARE = 0.5
RUN_ON_PROBABILITY = 0.25

# Jamo confusions Korean ASR makes on unfamiliar words. Indices follow the
# Unicode Hangul syllable layout (initial 19, medial 21, final 28).
_HANGUL_BASE = 0xAC00
_INITIAL_CONFUSIONS = {
    0: (15, 1),
    15: (0,),
    1: (0,),
    3: (16, 4),
    16: (3,),
    4: (3,),
    7: (17, 8),
    17: (7,),
    8: (7,),
    12: (14, 13),
    14: (12,),
    13: (12,),
    9: (10,),
    10: (9,),
    11: (18,),
    18: (11,),
    2: (5,),
    5: (2,),
}
_MEDIAL_CONFUSIONS = {
    1: (5,),
    5: (1,),
    3: (7,),
    7: (3,),
    4: (8,),
    8: (4,),
    18: (13,),
    13: (18,),
    6: (12,),
    12: (6,),
    10: (11, 15),
    11: (10, 15),
    15: (11,),
    19: (20,),
    16: (20,),
}
_FINAL_CONFUSIONS = {4: (21, 0), 21: (4, 0), 16: (21,), 8: (0,), 1: (0,), 0: (4,)}


def _jamo(syllable: str) -> tuple[int, int, int] | None:
    code = ord(syllable) - _HANGUL_BASE
    if not 0 <= code < 11172:
        return None
    return code // 588, (code % 588) // 28, code % 28


def _syllable(initial: int, medial: int, final: int) -> str:
    return chr(_HANGUL_BASE + initial * 588 + medial * 28 + final)


def phonetic_variant(surface: str, rng: random.Random) -> str | None:
    """One ASR-like edit: a jamo confusion, or a space added or dropped."""

    edits: list[str] = []
    for index, character in enumerate(surface):
        parts = _jamo(character)
        if parts is None:
            continue
        initial, medial, final = parts
        for option in _INITIAL_CONFUSIONS.get(initial, ()):
            edits.append(
                surface[:index]
                + _syllable(option, medial, final)
                + surface[index + 1 :]
            )
        for option in _MEDIAL_CONFUSIONS.get(medial, ()):
            edits.append(
                surface[:index]
                + _syllable(initial, option, final)
                + surface[index + 1 :]
            )
        # A final consonant is only added or dropped on non-initial syllables;
        # changing the first syllable's coda turns most words into new words.
        if index > 0:
            for option in _FINAL_CONFUSIONS.get(final, ()):
                edits.append(
                    surface[:index]
                    + _syllable(initial, medial, option)
                    + surface[index + 1 :]
                )
    syllables = [i for i, c in enumerate(surface) if _jamo(c) is not None]
    if " " in surface:
        edits.append(surface.replace(" ", "", 1))
    elif len(syllables) >= 3:
        split = rng.choice(syllables[1:-1])
        edits.append(surface[:split] + " " + surface[split:])
    edits = [edit for edit in edits if edit != surface]
    return rng.choice(edits) if edits else None


def _variant(
    surface: str, options: tuple[str, ...], blocked: set[str], rng: random.Random
) -> str:
    if rng.random() < PROCEDURAL_SHARE:
        for _ in range(6):
            candidate = phonetic_variant(surface, rng)
            if candidate is not None and candidate not in blocked:
                return candidate
    return rng.choice(options)


def _corrupt(
    text: str, meeting: Meeting, rng: random.Random, generated: Generated
) -> str:
    """Replace some correct occurrences with ASR variants, longest surface first."""

    variants: dict[str, tuple[tuple[str, ...], str]] = {}
    for surface, options in meeting.term_variants.items():
        variants[surface] = (options, "term")
    for surface, options in meeting.background_variants.items():
        variants[surface] = (options, "background")
    for surface, options in meeting.name_variants.items():
        variants[surface] = (options, "name")
    for surface, options in meeting.unlisted_variants.items():
        variants[surface] = (options, "unlisted")
    # A variant must not collide with a real word the meeting also uses or
    # with another protected surface, or the gold would be ambiguous.
    blocked = (
        set(meeting.distractors)
        | set(variants)
        | {person.name for person in meeting.people}
    )
    pattern = re.compile(
        "|".join(
            re.escape(surface) for surface in sorted(variants, key=len, reverse=True)
        )
    )
    protected = [
        match.span()
        for distractor in meeting.distractors
        for match in re.finditer(re.escape(distractor), text)
    ]
    pieces: list[str] = []
    cursor = 0
    for match in pattern.finditer(text):
        if any(start < match.end() and match.start() < end for start, end in protected):
            continue
        surface = match.group(0)
        options, kind = variants[surface]
        pieces.append(text[cursor : match.start()])
        chance = (
            UNLISTED_CORRUPT_PROBABILITY if kind == "unlisted" else CORRUPT_PROBABILITY
        )
        written = surface
        if rng.random() < chance:
            written = _variant(surface, options, blocked, rng)
            suffix = (
                HONORIFIC
                if kind == "name" and text.startswith(HONORIFIC, match.end())
                else ""
            )
            generated.lexical_errors.append(
                LexicalError(written + suffix, surface + suffix, kind)
            )
        if kind == "unlisted":
            generated.clusters.setdefault(surface, set()).add(written)
        pieces.append(written)
        cursor = match.end()
    pieces.append(text[cursor:])
    return "".join(pieces)


def _segments(text: str, rng: random.Random) -> list[str]:
    """Split a turn into ASR-sized segments; some long turns stay one run-on line."""

    segments: list[str] = []
    for sentence in SENTENCE_END.split(text.strip()):
        current = ""
        for word in sentence.split():
            candidate = f"{current} {word}".strip()
            if current and len(candidate) > SEGMENT_MAX_CHARS and current.endswith(","):
                segments.append(current)
                current = word
            else:
                current = candidate
        if current:
            segments.append(current)
    if len(segments) >= 3 and rng.random() < RUN_ON_PROBABILITY:
        return [" ".join(segment.rstrip(".") for segment in segments)]
    return segments


def generate(meeting: Meeting, seed: str = SEED) -> Generated:
    rng = random.Random(f"{seed}:{meeting.id}")
    order: list[str] = []
    for key, _ in meeting.turns:
        if key not in order:
            order.append(key)
    numbers = list(range(len(order)))
    rng.shuffle(numbers)
    labels = {
        key: f"SPEAKER_{number:02d}" for key, number in zip(order, numbers, strict=True)
    }
    people = {person.key: person for person in meeting.people}
    speaker_names = {
        labels[key]: (people[key].name if people[key].on_roster else None)
        for key in order
    }
    generated = Generated(meeting, "", labels, speaker_names)

    # Each turn: list of (label, segment text).
    turns: list[list[tuple[str, str]]] = []
    for key, text in meeting.turns:
        noisy = _corrupt(text, meeting, rng, generated)
        turns.append([(labels[key], segment) for segment in _segments(noisy, rng)])

    # Speaker-boundary errors: the tail of a turn's last segment is heard as
    # the next speaker, which is what overlapping diarization regions produce.
    candidates = [
        index
        for index in range(len(turns) - 1)
        if turns[index][-1][0] != turns[index + 1][0][0]
        and len(turns[index][-1][1].split()) >= 4
    ]
    rng.shuffle(candidates)
    boundary_turns = set(candidates[:BOUNDARY_ERRORS_PER_MEETING])

    lines: list[str] = []
    clock = 0.0
    for index, turn in enumerate(turns):
        for position, (label, segment) in enumerate(turn):
            is_split = index in boundary_turns and position == len(turn) - 1
            if not is_split:
                lines.append(f"[{label}] ({clock:.0f}s) {segment}")
                clock += len(segment) / CHARS_PER_SECOND + 0.4
                continue
            words = segment.split()
            cut = max(2, len(words) // 2)
            head, tail = " ".join(words[:cut]), " ".join(words[cut:])
            lines.append(f"[{label}] ({clock:.0f}s) {head}")
            clock += max(1.0, len(head) / CHARS_PER_SECOND)
            shown = turns[index + 1][0][0]
            second = round(clock)
            lines.append(f"[{shown}] ({second}s) {tail}")
            generated.boundary_errors.append(BoundaryError(second, shown, label))
            clock += len(tail) / CHARS_PER_SECOND + 0.4
        # A short reply to a question, stamped with the asker's label or
        # left unassigned; the next speaker actually said it.
        asker = turn[-1][0]
        if (
            index + 1 < len(turns)
            and turn[-1][1].endswith("?")
            and turns[index + 1][0][0] != asker
            and index not in boundary_turns
            and rng.random() < BACKCHANNEL_PROBABILITY
        ):
            shown = "UNKNOWN" if rng.random() < UNKNOWN_SHARE else asker
            second = round(clock)
            reply = rng.choice(BACKCHANNELS)
            lines.append(f"[{shown}] ({second}s) {reply}")
            generated.boundary_errors.append(
                BoundaryError(second, shown, turns[index + 1][0][0])
            )
            clock += len(reply) / CHARS_PER_SECOND + 0.4
        clock += 0.4
    generated.transcript = "\n".join(lines) + "\n"
    generated.line_count = len(lines)
    return generated


def corpus(seed: str = SEED) -> list[Generated]:
    return [generate(meeting, seed) for meeting in MEETINGS]
