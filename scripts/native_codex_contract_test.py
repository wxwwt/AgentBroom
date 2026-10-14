import json
import os
import unittest
from pathlib import Path
from unittest.mock import patch
import native_codex_contract as contract


class HelperTests(unittest.TestCase):
    def test_generated_home_ignores_inherited_secrets_and_roots(self):
        with patch.dict(
            os.environ,
            {
                "HOME": "/not-a-test-home",
                "CODEX_HOME": "/not-a-test-source",
                "OPENAI_API_KEY": "fake-do-not-inherit",
                "HTTPS_PROXY": "http://invalid",
                "SSLKEYLOGFILE": "/not-a-test-log",
                "CODEX_EXEC_SERVER_URL": "http://invalid",
            },
        ):
            home = contract.GeneratedHome()
        try:
            self.assertEqual(home.root, home.root.resolve())
            self.assertEqual(home.environment["CODEX_HOME"], str(home.codex))
            for key in [
                "OPENAI_API_KEY",
                "HTTPS_PROXY",
                "SSLKEYLOGFILE",
                "CODEX_EXEC_SERVER_URL",
            ]:
                self.assertNotIn(key, home.environment)
            for key in [
                "HOME",
                "CODEX_HOME",
                "TMPDIR",
                "XDG_CONFIG_HOME",
                "XDG_CACHE_HOME",
                "XDG_DATA_HOME",
            ]:
                self.assertTrue(Path(home.environment[key]).is_relative_to(home.root))
        finally:
            home.close()
        self.assertFalse(home.root.exists())

    def fake_client(self, frames):
        client = object.__new__(contract.NativeServer)
        client.next_id = 0
        client.frames = frames
        client.notifications = []
        client.send = lambda frame: None
        client.pump = lambda deadline: self.fail("unexpected transport read")
        return client

    def test_server_request_id_cannot_masquerade_as_response(self):
        c = self.fake_client(
            [{"id": 1, "method": "item/commandExecution/requestApproval", "params": {}}]
        )
        with self.assertRaisesRegex(contract.ContractError, "server request"):
            c.request("thread/delete", {})

    def test_notification_before_response_is_preserved(self):
        c = self.fake_client(
            [
                {"method": "thread/deleted", "params": {"threadId": contract.ROOT_ID}},
                {"id": 1, "result": {}},
            ]
        )
        self.assertEqual(c.request("thread/delete", {}), {})
        self.assertEqual(c.deleted(1), [contract.ROOT_ID])

    def test_native_error_is_not_success(self):
        c = self.fake_client(
            [{"id": 1, "error": {"code": -32600, "message": "blocked"}}]
        )
        with self.assertRaises(contract.RpcError):
            c.request("thread/delete", {})

    def test_missing_result_cannot_be_a_response(self):
        c = self.fake_client([{"id": 1}])
        with self.assertRaisesRegex(contract.ContractError, "no result"):
            c.request("thread/delete", {})

    def test_unindexed_new_rollout_is_not_preservation_success(self):
        home = contract.GeneratedHome()
        try:
            home.rollout(contract.ROOT_ID)
            home.rollout(contract.KEEP_ID)
            before = contract.files(home)
            home.expected_removed_files = {
                str(home.paths[contract.ROOT_ID].relative_to(home.codex))
            }

            class Server:
                def request(self, method, params):
                    home.paths[contract.ROOT_ID].unlink()
                    home.rollout(contract.FORK_ID)
                    return {}

                def deleted(self, count):
                    return [contract.ROOT_ID]

            rows = {contract.ROOT_ID: {}, contract.KEEP_ID: {}}
            with patch.object(
                home, "rows", return_value={contract.KEEP_ID: {}}
            ), patch.object(home, "spawn_edges", return_value=[]):
                with self.assertRaisesRegex(
                    contract.ContractError, "unexpected new file"
                ):
                    contract.deleted_verified(
                        Server(), home, [contract.ROOT_ID], rows, before, []
                    )
        finally:
            home.close()


if __name__ == "__main__":
    unittest.main()
