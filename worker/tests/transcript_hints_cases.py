"""Correction-evidence cases: what the refinement request points the model at."""

import unittest

from ..galpi_worker.minutes_pipeline import (
    MinutesContext,
    TranscriptChunk,
    build_map_messages,
    build_reduce_messages,
)
from ..galpi_worker.minutes_prompt import GlossaryEntry, Participant, plan_refinement
from ..galpi_worker.transcript_hints import analyze, distance


def person(name: str) -> Participant:
    return Participant(name=name, team=None, role=None, description=None, aliases=())


def glossary(*terms: str) -> list[GlossaryEntry]:
    return [GlossaryEntry(term=term, description=None) for term in terms]


class LexicalHintTests(unittest.TestCase):
    def test_points_misheard_terms_at_their_registered_spelling(self) -> None:
        # Given a jamo confusion, a spacing split, and a Latin term said aloud
        transcript = (
            "[SPEAKER_00] (0s) 웹서켓이 자꾸 끊겨요.\n"
            "[SPEAKER_01] (4s) 웹 소켓 재연결은 레디스에 상태를 두면 됩니다.\n"
            "[SPEAKER_00] (9s) 웹서켓도 같이 볼게요.\n"
        )

        # When
        block = analyze(transcript, "", [], glossary("웹소켓", "Redis")).block

        # Then
        # a repeated misspelling says how often it occurs
        self.assertIn('- 용어: "웹서켓" → "웹소켓" (2회)\n', block)
        self.assertIn('- 용어: "웹 소켓" → "웹소켓"', block)
        self.assertIn('- 용어: "레디스" → "Redis"', block)

    def test_correct_and_partial_mentions_are_not_candidates(self) -> None:
        # Given the registered spelling, a suffixed form, and one word of a
        # multi-word term
        transcript = (
            "[SPEAKER_00] (0s) 웹소켓 연결은 괜찮아요.\n"
            "[SPEAKER_01] (3s) 웹소켓별로 나눠서 보고, 대시보드만 고칠게요.\n"
        )

        # When
        block = analyze(transcript, "", [], glossary("웹소켓", "대시보드 화면")).block

        # Then
        self.assertNotIn("용어", block)

    def test_misheard_name_needs_an_addressing_context(self) -> None:
        # Given the same misspelling once as a plain word, once with 님
        transcript = (
            "[SPEAKER_00] (0s) 노리 끝에 정리됐어요.\n"
            "[SPEAKER_00] (3s) 노리 님은 결산 맡아 주세요.\n"
        )

        # When
        block = analyze(transcript, "", [person("누리")], []).block

        # Then
        self.assertEqual(block.count('- 이름: "노리" → "누리"'), 1)

    def test_compares_pronunciation_across_syllables(self) -> None:
        # A final consonant before ㅇ is heard as the next initial.
        self.assertEqual(distance("음악", "으막"), 0.0)
        self.assertGreater(distance("음악", "으낙"), 0.0)
        # The written form still counts when liaison would hide the match.
        self.assertLess(distance("북엔드", "북핸드"), distance("부겐드", "북핸드"))
        # A loanword's inserted ㅡ syllable is the previous syllable's final.
        self.assertEqual(distance("부크마크", "북마크"), 0.0)


class SpeakerMappingTests(unittest.TestCase):
    def test_maps_the_addressed_speaker_and_eliminates_the_last(self) -> None:
        # Given two roster members and one of them addressed by name
        transcript = (
            "[SPEAKER_00] (0s) 누리 님, 결산 어떻게 됐어요?\n"
            "[SPEAKER_01] (3s) 거의 끝났습니다.\n"
        )

        # When
        block = analyze(transcript, "", [person("누리"), person("태오")], []).block

        # Then
        # each mapping names the evidence the model should check
        self.assertIn("- 화자: SPEAKER_01 → 누리 (근거: 0s 호명)", block)
        self.assertIn(
            "- 화자: SPEAKER_00 → 태오 (근거: 나머지 화자와 명단 대조)", block
        )

    def test_leaves_a_label_unmapped_when_a_guest_may_hold_it(self) -> None:
        # Given three labels and a two-person roster
        transcript = (
            "[SPEAKER_00] (0s) 누리 님, 결산 어떻게 됐어요?\n"
            "[SPEAKER_01] (3s) 거의 끝났습니다.\n"
            "[SPEAKER_02] (6s) 저희 쪽 자료도 보내 드릴게요.\n"
        )

        # When
        block = analyze(transcript, "", [person("누리"), person("태오")], []).block

        # Then
        self.assertIn("- 화자: SPEAKER_01 → 누리", block)
        self.assertNotIn("SPEAKER_00 →", block)
        self.assertNotIn("SPEAKER_02 →", block)


class BoundaryHintTests(unittest.TestCase):
    def test_flags_a_reply_stamped_with_the_askers_label(self) -> None:
        # Given
        transcript = (
            "[SPEAKER_00] (10s) 오늘 배포하죠?\n"
            "[SPEAKER_00] (12s) 네.\n"
            "[SPEAKER_01] (13s) 오후에 올리겠습니다.\n"
        )

        # When
        hints = analyze(transcript, "", [], [])

        # Then
        self.assertIn(
            "- 화자 경계: (12s) SPEAKER_00 → SPEAKER_01"
            " (근거: 10s 질문자 라벨이 붙은 짧은 응답)",
            hints.block,
        )

    def test_flags_a_sentence_tail_cut_into_the_next_turn(self) -> None:
        # Given
        transcript = (
            "[SPEAKER_00] (20s) 정산서는 내일\n"
            "[SPEAKER_01] (22s) 오전까지 보내 드릴게요.\n"
            "[SPEAKER_01] (25s) 저는 쿠폰 쪽을 볼게요.\n"
        )

        # When
        hints = analyze(transcript, "", [], [])

        # Then
        self.assertIn(
            "- 화자 경계: (22s) SPEAKER_01 → SPEAKER_00"
            " (근거: 20s 줄이 문장 중간에서 끊김)",
            hints.block,
        )
        # The flagged part opens its own line, so the model can find it, and
        # what follows keeps an inline timestamp marking where it ends.
        self.assertIn(
            "[SPEAKER_00] (20s) 정산서는 내일\n"
            "[SPEAKER_01] (22s) 오전까지 보내 드릴게요. (25s) 저는 쿠폰 쪽을 볼게요.\n",
            hints.transcript,
        )

    def test_conversation_trailing_off_is_not_a_diarization_error(self) -> None:
        # Given lines that end unfinished for ordinary conversational reasons:
        # a filler, an interjection after which the first speaker resumes,
        # a hesitation after one's own question, and a backchannel.
        transcript = (
            "[SPEAKER_00] (0s) 정산서는 내일 그\n"
            "[SPEAKER_01] (1s) 쿠폰 쪽도 같이 봐야 해요.\n"
            "[SPEAKER_00] (5s) 결산이 늦어진 이유는\n"
            "[SPEAKER_01] (6s) 시스템 문제예요?\n"
            "[SPEAKER_00] (8s) 아니요, 입력이 늦었어요.\n"
            "[SPEAKER_01] (12s) 다음 주에 배포하죠?\n"
            "[SPEAKER_01] (14s) 음\n"
            "[SPEAKER_00] (15s) 수요일 오후로 잡을게요.\n"
            "[SPEAKER_00] (20s) 정산서 양식을 바꾸면\n"
            "[SPEAKER_01] (21s) 맞아요, 그게 맞아요.\n"
            "[SPEAKER_00] (23s) 검토가 빨라질 거예요.\n"
        )

        # When
        hints = analyze(transcript, "", [], [])

        # Then
        self.assertNotIn("화자 경계", hints.block)

    def test_merges_one_speakers_consecutive_lines_without_losing_words(self) -> None:
        # Given
        transcript = (
            "[SPEAKER_00] (0s) 결산은 끝났고요.\n"
            "[SPEAKER_00] (2s) 정산서도 보냈습니다.\n"
            "[SPEAKER_01] (5s) 확인할게요.\n"
        )

        # When
        compacted = analyze(transcript, "", [], []).transcript

        # Then
        self.assertEqual(
            compacted,
            "[SPEAKER_00] (0s) 결산은 끝났고요. 정산서도 보냈습니다.\n"
            "[SPEAKER_01] (5s) 확인할게요.\n",
        )


class RequestEvidenceTests(unittest.TestCase):
    TRANSCRIPT = "[SPEAKER_00] (0s) 웹서켓이 자꾸 끊겨요.\n"

    def test_every_request_path_carries_the_same_candidates(self) -> None:
        # When
        single = plan_refinement(
            self.TRANSCRIPT, "", [], glossary("웹소켓"), None, "single"
        )
        mapped = plan_refinement(
            self.TRANSCRIPT, "", [], glossary("웹소켓"), None, "map_reduce"
        )

        # Then
        for plan in (single, mapped):
            content = plan.requests()[0][1]["content"]
            self.assertIn(
                '<보정후보>\n- 용어: "웹서켓" → "웹소켓"\n</보정후보>', content
            )

    def test_omits_the_block_when_nothing_looks_wrong(self) -> None:
        # When
        plan = plan_refinement(self.TRANSCRIPT, "", [], [], None, "single")

        # Then
        self.assertNotIn("보정후보>", plan.requests()[0][1]["content"])

    def test_each_map_chunk_gets_only_the_candidates_it_can_use(self) -> None:
        # Given whole-meeting candidates and a chunk that holds one of them
        block = (
            '- 용어: "웹서켓" → "웹소켓"\n'
            '- 용어: "래디스" → "Redis"\n'
            "- 화자: SPEAKER_01 → 누리\n"
            "- 화자 경계: (12s) SPEAKER_00 → SPEAKER_01\n"
            "- 화자 경계: (95s) SPEAKER_01 → SPEAKER_00"
        )
        context = MinutesContext("", "", "", None, corrections=block)
        chunk = TranscriptChunk(1, 2, "[SPEAKER_00] (12s) 네, 웹서켓이요.")

        # When
        content = build_map_messages(chunk, context)[1]["content"]
        reduced = build_reduce_messages(["노트"], context, "system")[1]["content"]

        # Then the chunk keeps its own spelling, every speaker mapping, and
        # only boundaries inside it; the reduce pass sees everything.
        self.assertIn(
            '<보정후보>\n- 용어: "웹서켓" → "웹소켓"\n- 화자: SPEAKER_01 → 누리\n'
            "- 화자 경계: (12s) SPEAKER_00 → SPEAKER_01\n</보정후보>",
            content,
        )
        self.assertIn(block, reduced)
