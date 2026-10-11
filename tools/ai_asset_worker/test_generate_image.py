"""Offline protocol regressions. No provider credentials or paid requests."""
import base64
import unittest
from unittest.mock import patch

import generate_image as worker


class ImageContracts(unittest.TestCase):
    def test_image_output_is_discovered_not_inferred_from_the_name(self):
        model = {"id": "vendor/ordinary-name", "architecture": {"output_modalities": ["image"]}}
        with patch.object(worker, "provider_json", side_effect=[{"data": []}, {"data": [model]}]):
            protocol, selected = worker.openrouter_image_contract("https://example.invalid", "unused", model["id"])
        self.assertEqual(protocol, "images")
        self.assertEqual(selected["id"], model["id"])
        with patch.object(worker, "provider_json", side_effect=[{"data": []}, {"data": []}]):
            with self.assertRaisesRegex(RuntimeError, "does not advertise image output"):
                worker.openrouter_image_contract("https://example.invalid", "unused", "vendor/image-in-name")

    def test_multimodal_model_keeps_its_identity_and_chat_protocol(self):
        model = {"id": "selected-model", "architecture": {"output_modalities": ["text", "image"]}}
        with patch.object(worker, "provider_json", side_effect=[{"data": [model]}, {"data": []}]):
            protocol, selected = worker.openrouter_image_contract("https://example.invalid", "unused", "selected-model")
        self.assertEqual(protocol, "chat/completions")
        self.assertEqual(selected["id"], "selected-model")

    def test_multimodal_image_parts_decode_without_downloading_a_url_from_prose(self):
        data = b"test image bytes"
        result = {"choices": [{"message": {"images": [{"image_url": {
            "url": "data:image/png;base64," + base64.b64encode(data).decode("ascii")
        }}]}}]}
        self.assertEqual(worker.fetch_image_bytes(result), data)
        with self.assertRaisesRegex(RuntimeError, "no image data"):
            worker.fetch_image_bytes({"choices": [{"message": {"content": "Look at https://example.invalid"}}]})


if __name__ == "__main__":
    unittest.main()
