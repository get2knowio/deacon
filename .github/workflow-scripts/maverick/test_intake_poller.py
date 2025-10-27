#!/usr/bin/env python3

"""
Unit tests for intake_poller.py

Run with: python3 -m pytest test_intake_poller.py -v
Or with unittest: python3 -m unittest test_intake_poller.py -v
"""

import json
import os
import sys
import unittest
from typing import Any, Dict, List
from unittest.mock import MagicMock, Mock, call, patch

# Import the module under test
import intake_poller


class TestNormalize(unittest.TestCase):
    """Tests for the normalize function."""

    def test_normalize_basic(self):
        """Test basic normalization removes emoji and punctuation."""
        self.assertEqual(intake_poller.normalize("Ready ✅"), "ready ")
        self.assertEqual(intake_poller.normalize("In Flight 🚀"), "in flight ")

    def test_normalize_punctuation(self):
        """Test normalization removes various punctuation."""
        self.assertEqual(intake_poller.normalize("Ready-to-Go!"), "readytogo")
        self.assertEqual(intake_poller.normalize("Status: Done."), "status done")

    def test_normalize_preserves_spaces(self):
        """Test normalization preserves internal spaces."""
        self.assertEqual(intake_poller.normalize("In Flight"), "in flight")
        self.assertEqual(intake_poller.normalize("Ready for Takeoff"), "ready for takeoff")

    def test_normalize_lowercase(self):
        """Test normalization converts to lowercase."""
        self.assertEqual(intake_poller.normalize("UPPERCASE"), "uppercase")
        self.assertEqual(intake_poller.normalize("MiXeD CaSe"), "mixed case")


class TestGhGraphql(unittest.TestCase):
    """Tests for the gh_graphql function."""

    @patch.dict(os.environ, {}, clear=True)
    def test_gh_graphql_no_token(self):
        """Test gh_graphql raises SystemExit when GH_TOKEN is not set."""
        with self.assertRaises(SystemExit) as cm:
            intake_poller.gh_graphql("query { viewer { login } }")
        self.assertIn("GH_TOKEN not set", str(cm.exception))

    @patch.dict(os.environ, {"GH_TOKEN": "test_token"})
    @patch("intake_poller.request.urlopen")
    def test_gh_graphql_success(self, mock_urlopen):
        """Test gh_graphql successfully makes API call."""
        mock_response = Mock()
        mock_response.read.return_value = b'{"data": {"viewer": {"login": "test"}}}'
        mock_urlopen.return_value.__enter__.return_value = mock_response

        result = intake_poller.gh_graphql("query { viewer { login } }")
        
        self.assertEqual(result, '{"data": {"viewer": {"login": "test"}}}')
        mock_urlopen.assert_called_once()

    @patch.dict(os.environ, {"GH_TOKEN": "test_token"})
    @patch("intake_poller.request.urlopen")
    def test_gh_graphql_coerces_number(self, mock_urlopen):
        """Test gh_graphql coerces 'number' parameter to int."""
        mock_response = Mock()
        mock_response.read.return_value = b'{"data": {}}'
        mock_urlopen.return_value.__enter__.return_value = mock_response

        intake_poller.gh_graphql("query($number:Int!){}", number="42")
        
        # Verify the request was made with number as int
        call_args = mock_urlopen.call_args
        request_obj = call_args[0][0]
        body = json.loads(request_obj.data.decode('utf-8'))
        self.assertEqual(body['variables']['number'], 42)
        self.assertIsInstance(body['variables']['number'], int)

    @patch.dict(os.environ, {"GH_TOKEN": "test_token"})
    @patch("intake_poller.request.urlopen")
    def test_gh_graphql_http_error(self, mock_urlopen):
        """Test gh_graphql handles HTTP errors."""
        from urllib.error import HTTPError
        
        mock_response = Mock()
        mock_response.read.return_value = b'{"error": "Not found"}'
        mock_urlopen.side_effect = HTTPError(
            url="https://api.github.com/graphql",
            code=404,
            msg="Not Found",
            hdrs=None,  # type: ignore
            fp=mock_response
        )

        with self.assertRaises(SystemExit) as cm:
            intake_poller.gh_graphql("query { viewer { login } }")
        self.assertIn("404", str(cm.exception))


class TestFetchAllItems(unittest.TestCase):
    """Tests for the fetch_all_items function."""

    @patch("intake_poller.gh_graphql")
    def test_fetch_all_items_single_page(self, mock_gh_graphql):
        """Test fetching items with single page (no pagination)."""
        mock_gh_graphql.return_value = json.dumps({
            "data": {
                "node": {
                    "items": {
                        "nodes": [
                            {"id": "item1", "content": {"__typename": "Issue", "number": 1}},
                            {"id": "item2", "content": {"__typename": "Issue", "number": 2}},
                        ],
                        "pageInfo": {"hasNextPage": False, "endCursor": None}
                    }
                }
            }
        })

        result = intake_poller.fetch_all_items("project_id_123")
        
        self.assertEqual(len(result), 2)
        self.assertEqual(result[0]["id"], "item1")
        self.assertEqual(result[1]["id"], "item2")
        mock_gh_graphql.assert_called_once()

    @patch("intake_poller.gh_graphql")
    def test_fetch_all_items_multiple_pages(self, mock_gh_graphql):
        """Test fetching items with pagination."""
        # First page
        mock_gh_graphql.side_effect = [
            json.dumps({
                "data": {
                    "node": {
                        "items": {
                            "nodes": [{"id": "item1"}],
                            "pageInfo": {"hasNextPage": True, "endCursor": "cursor1"}
                        }
                    }
                }
            }),
            # Second page
            json.dumps({
                "data": {
                    "node": {
                        "items": {
                            "nodes": [{"id": "item2"}],
                            "pageInfo": {"hasNextPage": False, "endCursor": None}
                        }
                    }
                }
            }),
        ]

        result = intake_poller.fetch_all_items("project_id_123")
        
        self.assertEqual(len(result), 2)
        self.assertEqual(result[0]["id"], "item1")
        self.assertEqual(result[1]["id"], "item2")
        self.assertEqual(mock_gh_graphql.call_count, 2)

    @patch("intake_poller.gh_graphql")
    def test_fetch_all_items_graphql_error(self, mock_gh_graphql):
        """Test fetch_all_items handles GraphQL errors."""
        mock_gh_graphql.return_value = json.dumps({
            "errors": [{"message": "Something went wrong"}]
        })

        with self.assertRaises(SystemExit) as cm:
            intake_poller.fetch_all_items("project_id_123")
        self.assertIn("Something went wrong", str(cm.exception))


class TestPreflightCheck(unittest.TestCase):
    """Tests for the preflight_check function."""

    @patch("intake_poller.gh_graphql")
    def test_preflight_check_success(self, mock_gh_graphql):
        """Test preflight_check passes with valid token and access."""
        mock_gh_graphql.side_effect = [
            # Viewer query
            json.dumps({"data": {"viewer": {"login": "testuser"}}}),
            # Project access query
            json.dumps({"data": {"organization": {"projectV2": {"id": "proj123"}}}}),
        ]

        # Should not raise
        intake_poller.preflight_check("test-org", "1")
        
        self.assertEqual(mock_gh_graphql.call_count, 2)

    @patch("intake_poller.gh_graphql")
    def test_preflight_check_invalid_token(self, mock_gh_graphql):
        """Test preflight_check fails with invalid token."""
        mock_gh_graphql.return_value = json.dumps({"data": {}})

        with self.assertRaises(SystemExit) as cm:
            intake_poller.preflight_check("test-org", "1")
        self.assertIn("invalid", str(cm.exception).lower())

    @patch("intake_poller.gh_graphql")
    def test_preflight_check_no_project_access(self, mock_gh_graphql):
        """Test preflight_check fails without project access."""
        mock_gh_graphql.side_effect = [
            # Viewer query succeeds
            json.dumps({"data": {"viewer": {"login": "testuser"}}}),
            # Project access query fails
            json.dumps({"errors": [{"message": "Resource not accessible"}]}),
        ]

        with self.assertRaises(SystemExit) as cm:
            intake_poller.preflight_check("test-org", "1")
        self.assertIn("Resource not accessible", str(cm.exception))


class TestGhCommand(unittest.TestCase):
    """Tests for the gh_command function."""

    @patch("intake_poller.subprocess.check_call")
    def test_gh_command_basic(self, mock_check_call):
        """Test gh_command executes with correct arguments."""
        intake_poller.gh_command("issue", "list")
        
        mock_check_call.assert_called_once_with(["gh", "issue", "list"])

    @patch("intake_poller.subprocess.check_call")
    def test_gh_command_with_flags(self, mock_check_call):
        """Test gh_command handles flags correctly."""
        intake_poller.gh_command("issue", "comment", "123", "--body", "test comment")
        
        mock_check_call.assert_called_once_with(
            ["gh", "issue", "comment", "123", "--body", "test comment"]
        )


class TestMainIntegration(unittest.TestCase):
    """Integration tests for the main function."""

    @patch.dict(os.environ, {
        "ORG": "test-org",
        "PROJECT_NUMBER": "1",
        "STATUS_READY": "Ready for Takeoff",
        "STATUS_INFLIGHT": "In Flight",
        "GITHUB_REPOSITORY": "test-owner/test-repo",
    })
    @patch("intake_poller.preflight_check")
    @patch("intake_poller.fetch_all_items")
    @patch("intake_poller.gh_graphql")
    def test_main_no_candidates(self, mock_gh_graphql, mock_fetch, mock_preflight):
        """Test main function when no items are in Ready status."""
        # Mock project query
        mock_gh_graphql.return_value = json.dumps({
            "data": {
                "organization": {
                    "projectV2": {
                        "id": "proj123",
                        "fields": {
                            "nodes": [
                                {
                                    "id": "field1",
                                    "name": "Status",
                                    "options": [
                                        {"id": "opt1", "name": "Ready for Takeoff"},
                                        {"id": "opt2", "name": "In Flight"},
                                    ]
                                }
                            ]
                        }
                    }
                }
            }
        })
        
        # No items match
        mock_fetch.return_value = []

        with patch("builtins.print") as mock_print:
            intake_poller.main()
            mock_print.assert_any_call("No items in Ready for Takeoff for this repository.")

    @patch.dict(os.environ, {
        "ORG": "test-org",
        "PROJECT_NUMBER": "1",
        "STATUS_READY": "Ready for Takeoff",
        "STATUS_INFLIGHT": "In Flight",
        "GITHUB_REPOSITORY": "test-owner/test-repo",
    })
    @patch("intake_poller.preflight_check")
    @patch("intake_poller.fetch_all_items")
    @patch("intake_poller.gh_graphql")
    def test_main_gate_check_blocks_intake(self, mock_gh_graphql, mock_fetch, mock_preflight):
        """Test main function respects gate statuses."""
        # Mock project query
        mock_gh_graphql.return_value = json.dumps({
            "data": {
                "organization": {
                    "projectV2": {
                        "id": "proj123",
                        "fields": {
                            "nodes": [
                                {
                                    "id": "field1",
                                    "name": "Status",
                                    "options": [
                                        {"id": "opt1", "name": "Ready for Takeoff"},
                                        {"id": "opt2", "name": "In Flight"},
                                    ]
                                }
                            ]
                        }
                    }
                }
            }
        })
        
        # Item exists in "In Flight" status
        mock_fetch.return_value = [
            {
                "id": "item1",
                "content": {"__typename": "Issue", "number": 1},
                "fieldValues": {
                    "nodes": [
                        {
                            "field": {"id": "field1", "name": "Status"},
                            "optionId": "opt2",  # In Flight
                        }
                    ]
                }
            }
        ]

        with patch("builtins.print") as mock_print:
            intake_poller.main()
            mock_print.assert_any_call(
                "Active item already in progress (one of: In Flight, Debrief, Remediation, Verification, Ready for Integration). Skipping intake."
            )

    @patch.dict(os.environ, {}, clear=True)
    def test_main_missing_env_vars(self):
        """Test main function fails with missing environment variables."""
        with self.assertRaises(KeyError):
            intake_poller.main()


if __name__ == "__main__":
    unittest.main()
