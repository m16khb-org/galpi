import unittest
from pathlib import Path
from tempfile import TemporaryDirectory

from ..galpi_worker.preparation import link_ffmpeg
from ..galpi_worker.runtime import (
    ffmpeg_link_name,
    needs_cpu_fallback,
    select_torch_device,
)


class DeviceSelectionTests(unittest.TestCase):
    def test_cuda_wins_over_mps_and_cpu(self) -> None:
        # Given/When/Then: priority is cuda > mps > cpu
        self.assertEqual(
            select_torch_device(mps_available=True, cuda_available=True), "cuda"
        )
        self.assertEqual(select_torch_device(mps_available=True), "mps")
        self.assertEqual(select_torch_device(mps_available=False), "cpu")

    def test_fallback_applies_to_any_accelerator(self) -> None:
        self.assertTrue(needs_cpu_fallback("cuda"))
        self.assertTrue(needs_cpu_fallback("mps"))
        self.assertFalse(needs_cpu_fallback("cpu"))

    def test_ffmpeg_link_name_per_platform(self) -> None:
        self.assertEqual(ffmpeg_link_name(platform="win32"), "ffmpeg.exe")
        self.assertEqual(ffmpeg_link_name(platform="darwin"), "ffmpeg")

    def test_link_ffmpeg_creates_link_in_engine_bin(self) -> None:
        from unittest import mock

        with TemporaryDirectory() as raw:
            root = Path(raw)
            target = root / "real-ffmpeg"
            target.write_bytes(b"x")
            with mock.patch(
                "worker.galpi_worker.preparation.ffmpeg_executable",
                return_value=str(target),
            ):
                link_ffmpeg(root / "bin")
            self.assertTrue((root / "bin" / ffmpeg_link_name()).exists())


if __name__ == "__main__":
    unittest.main()
