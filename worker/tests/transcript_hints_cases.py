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

    def test_terms_ending_in_a_particle_like_syllable_still_match(self) -> None:
        # Given terms whose last syllable doubles as the particle 이
        transcript = (
            "[SPEAKER_00] (0s) 카카오패이 연동부터 볼게요.\n"
            "[SPEAKER_01] (4s) 에이아이 요약도 같이 붙이죠.\n"
        )

        # When
        block = analyze(transcript, "", [], glossary("카카오페이", "AI")).block

        # Then
        self.assertIn('- 용어: "카카오패이" → "카카오페이"', block)
        self.assertIn('- 용어: "에이아이" → "AI"', block)

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

    def test_ordinary_speech_does_not_match_terms(self) -> None:
        # Given a conjugated verb that sounds like a Latin term, a filler
        # next to a word, and a native word one sound from a Latin term
        transcript = (
            "[SPEAKER_00] (0s) 빚을 다 갚고 나서 바로 그 얘기를 했어요.\n"
            "[SPEAKER_01] (4s) 실력이 늘었네요.\n"
            "[SPEAKER_00] (8s) 카프카 설정은 제가 볼게요.\n"
        )

        # When
        block = analyze(transcript, "", [], glossary("Kafka", "Slack", "백로그")).block

        # Then only the term actually said is pointed at its registered spelling
        self.assertEqual(block, '- 용어: "카프카" → "Kafka"')

    def test_groups_an_unlisted_word_only_on_repeated_evidence(self) -> None:
        # Given two spellings of an unregistered word, each said once
        once = (
            "[SPEAKER_00] (0s) 오케스트레이터를 먼저 띄웁니다.\n"
            "[SPEAKER_01] (4s) 오케스트레이타는 다음 주에 바꿔요.\n"
        )
        # and the same meeting where one spelling comes back
        again = once + "[SPEAKER_00] (9s) 오케스트레이터 설정도 같이 볼게요.\n"

        # When
        sparse = analyze(once, "", [], []).block
        repeated = analyze(again, "", [], []).block

        # Then two one-off spellings may be two different words
        self.assertNotIn("같은 표현", sparse)
        self.assertIn('- 같은 표현: "오케스트레이타" = "오케스트레이터"', repeated)

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

    def test_a_topic_after_저는_is_not_a_self_introduction(self) -> None:
        # Given a roster name one sound from a hobby the speaker mentions
        transcript = (
            "[SPEAKER_00] (0s) 주말에 뭐 하세요?\n"
            "[SPEAKER_01] (3s) 저는 누리를 자주 가요.\n"
            "[SPEAKER_01] (6s) 저는 노리입니다.\n"
        )

        # When
        block = analyze(transcript, "", [person("누리"), person("태오")], []).block

        # Then only the introduction proper counts as evidence
        self.assertIn("- 화자: SPEAKER_01 → 누리 (근거: 6s 자기소개)", block)

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

    def test_a_reply_before_the_asker_resumes_goes_to_the_earlier_speaker(self) -> None:
        # Given a question answered with the asker's label, after which the
        # asker keeps talking
        transcript = (
            "[SPEAKER_01] (0s) 결산은 다 끝났어요.\n"
            "[SPEAKER_00] (4s) 정산서도 보냈죠?\n"
            "[SPEAKER_00] (6s) 네.\n"
            "[SPEAKER_00] (7s) 그럼 쿠폰 쪽 볼게요.\n"
        )

        # When
        hints = analyze(transcript, "", [], [])

        # Then the person the question was put to is the candidate
        self.assertIn(
            "- 화자 경계: (6s) SPEAKER_00 → SPEAKER_01"
            " (근거: 4s 질문자 라벨이 붙은 짧은 응답, 질문 직전 상대 화자(0s)로 추정)",
            hints.block,
        )

    def test_a_reply_names_both_candidates_when_they_differ(self) -> None:
        # Given the next speaker and the one before the question differ
        transcript = (
            "[SPEAKER_02] (0s) 결산은 다 끝났어요.\n"
            "[SPEAKER_00] (4s) 정산서도 보냈죠?\n"
            "[SPEAKER_00] (6s) 네.\n"
            "[SPEAKER_01] (7s) 쿠폰 쪽은 제가 볼게요.\n"
        )

        # When
        hints = analyze(transcript, "", [], [])

        # Then
        self.assertIn(
            "- 화자 경계: (6s) SPEAKER_00 → SPEAKER_01"
            " (근거: 4s 질문자 라벨이 붙은 짧은 응답, 질문 직전 상대 화자 SPEAKER_02일 수도 있음)",
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
