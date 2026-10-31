import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parents[1] / "src"))

import wayfinder


def test_parses_resource_actions_and_redacts_sensitive_before_and_after_values():
    plan = {
        "format_version": "1.2",
        "errored": False,
        "resource_changes": [
            {
                "address": "aws_instance.web",
                "change": {
                    "actions": ["delete", "create"],
                    "before": {"name": "old", "password": "old-secret"},
                    "after": {"name": "new", "password": "new-secret"},
                    "before_sensitive": {"password": True},
                    "after_sensitive": {"password": True},
                },
            }
        ],
    }

    resources, counts = wayfinder.parse_plan(plan)

    assert resources == [
        {
            "address": "aws_instance.web",
            "actions": ["delete", "create"],
            "before": {"name": "old", "password": "[REDACTED]"},
            "after": {"name": "new", "password": "[REDACTED]"},
        }
    ]
    assert counts == (1, 0, 1)
    assert "old-secret" not in str(resources)
    assert "new-secret" not in str(resources)


def test_plan_counts_create_update_delete_and_no_change_actions():
    plan: dict[str, object] = {
        "resource_changes": [
            {"address": "created", "change": {"actions": ["create"]}},
            {"address": "updated", "change": {"actions": ["update"]}},
            {"address": "deleted", "change": {"actions": ["delete"]}},
            {"address": "steady", "change": {"actions": ["no-op"]}},
        ]
    }

    resources, counts = wayfinder.parse_plan(plan)

    assert [resource["actions"] for resource in resources] == [
        ["create"], ["update"], ["delete"], ["no-op"]
    ]
    assert counts == (1, 1, 1)


def test_unknown_values_are_labeled_and_sensitive_values_stay_redacted():
    plan: dict[str, object] = {
        "resource_changes": [{
            "address": "aws_instance.web",
            "change": {
                "actions": ["create"],
                "before": None,
                "after": {"computed": None, "secret": None},
                "after_unknown": {"computed": True, "secret": True},
                "after_sensitive": {"secret": True},
            },
        }],
    }

    resources, _ = wayfinder.parse_plan(plan)

    assert resources[0]["after"] == {"computed": "[UNKNOWN]", "secret": "[REDACTED]"}


def test_errored_or_malformed_plan_is_rejected():
    try:
        wayfinder.parse_plan({"errored": True, "resource_changes": []})
    except ValueError:
        pass
    else:
        raise AssertionError("errored Terraform output must not be treated as a plan")


def test_plan_command_requests_isolated_native_and_json_outputs(tmp_path):
    command = wayfinder.build_plan_command(
        "terragrunt",
        tmp_path / "plans",
        tmp_path / "json",
    )

    assert command == [
        "terragrunt",
        "run",
        "--all",
        "--out-dir",
        str(tmp_path / "plans"),
        "--json-out-dir",
        str(tmp_path / "json"),
        "plan",
    ]
