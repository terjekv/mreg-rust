"""Keep the pinned CLI gap list from accepting unrelated validation failures."""

import importlib.util
from pathlib import Path
import unittest

SPEC = importlib.util.spec_from_file_location(
    "compat", Path(__file__).with_name("check-mreg-cli-compat.py")
)
compat = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(compat)


def sshfp_request(command="host sshfp_add bar 1 1 12345678abcde", status=400,
                  message="validation error: hex value must contain an even number of digits",
                  path="/api/v1/sshfps/"):
    return {"command": command, "api_requests": [{
        "method": "POST", "url": path, "status": status,
        "response": {"error": "validation_error", "message": message},
    }]}


class StrictValidationGaps(unittest.TestCase):
    def test_known_invalid_fixture_is_reported(self):
        self.assertEqual(compat.strict_validation_gap(sshfp_request()), "SSHFP encoding")

    def test_unlisted_command_is_not_accepted(self):
        self.assertIsNone(compat.strict_validation_gap(sshfp_request(command="host sshfp_add valid 1 1 aabb")))

    def test_arbitrary_validation_error_is_not_accepted(self):
        self.assertIsNone(compat.strict_validation_gap(sshfp_request(message="validation error: unrelated failure")))

    def test_wrong_status_is_not_accepted(self):
        self.assertIsNone(compat.strict_validation_gap(sshfp_request(status=500)))

    def test_wrong_endpoint_is_not_accepted(self):
        self.assertIsNone(compat.strict_validation_gap(sshfp_request(path="/api/v1/dhcps/")))


if __name__ == "__main__":
    unittest.main()
