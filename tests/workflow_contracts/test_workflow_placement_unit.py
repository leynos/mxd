"""What the placement reader accepts, and what it refuses to guess at.

The workflow contracts assert where every lane runs, and each of them rests on
one reader. A contract can therefore only be as trustworthy as that reader's
refusals: a shape read as something it is not passes every contract while the
lane runs somewhere nobody asserted.

This module drives ``runner_placement`` directly, with mappings rather than
files, because the shapes it must refuse are ones this repository does not
declare. A contract parametrized over the workflows as they stand exercises
only the accepted shapes, so it would pass unchanged with every guard here
deleted and could not have caught the defect these tests exist for.

The four shapes and the refusals are described in
:mod:`ci_workflow_placement`; each case below names the reason it is a case.
"""

from __future__ import annotations

import pytest
from ci_workflow_placement import runner_placement
from ci_workflow_reader import WorkflowShapeError

COORDINATE = "ci.yml:build-test"


class TestAcceptedShapes:
    """The three shapes the reader models, and the input it defers."""

    def test_a_bare_label_is_read_as_one_unconditional_label(self) -> None:
        """The common form: one literal, no guard, declaration kept verbatim."""
        placement = runner_placement({"runs-on": "ubuntu-latest"}, COORDINATE)
        assert placement.labels == ("ubuntu-latest",)
        assert placement.guard is None
        assert placement.declaration == "ubuntu-latest"
        assert placement.from_input is None

    def test_a_list_of_literal_labels_keeps_declaration_order(self) -> None:
        """Order is kept because a contract may assert which label leads."""
        placement = runner_placement(
            {"runs-on": ["self-hosted", "linux", "x64"]}, COORDINATE
        )
        assert placement.labels == ("self-hosted", "linux", "x64")
        assert placement.guard is None
        assert placement.declaration is None

    def test_a_conditional_placement_keeps_its_guard_and_both_arms(self) -> None:
        """The guarded arm leads, so a swapped pair is visible to a contract.

        The guard is kept as text because the arms alone cannot tell a fork
        fallback from an event-keyed placement, and those are different
        decisions with different costs.
        """
        declaration = (
            "${{ github.event.pull_request.head.repo.fork "
            "&& 'ubuntu-latest' || 'ubicloud-standard-2' }}"
        )
        placement = runner_placement({"runs-on": declaration}, COORDINATE)
        assert placement.labels == ("ubuntu-latest", "ubicloud-standard-2")
        assert placement.guard == "github.event.pull_request.head.repo.fork"
        assert placement.declaration == declaration

    def test_a_wrapped_expression_reads_the_same_as_a_single_line_one(self) -> None:
        """A folded scalar that failed to fold leaves a line break in the value.

        YAML hands the value over with whatever whitespace survived folding, so
        the reader matches whitespace rather than a single space. Without this
        a legitimate wrapped placement would be refused and the lane would have
        to be rewritten to satisfy the reader.
        """
        placement = runner_placement(
            {
                "runs-on": (
                    "${{ github.event.pull_request.head.repo.fork\n"
                    "  && 'ubuntu-latest' || 'ubicloud-standard-2' }}"
                )
            },
            COORDINATE,
        )
        assert placement.labels == ("ubuntu-latest", "ubicloud-standard-2")
        assert placement.guard == "github.event.pull_request.head.repo.fork"

    def test_a_placement_from_an_input_names_the_input_and_no_label(self) -> None:
        """A reusable workflow is pinned by its caller, not by itself.

        The record names no label, so a contract reading this file alone cannot
        mistake the callee for the place the decision is made.
        """
        placement = runner_placement({"runs-on": "${{ inputs.runner }}"}, COORDINATE)
        assert placement.labels == ()
        assert placement.from_input == "runner"
        assert placement.guard is None


class TestRefusedShapes:
    """Every shape the reader will not model, and the message that says so."""

    def test_an_expression_among_list_entries_is_refused(self) -> None:
        """The subtle case, and the one this guard exists for.

        GitHub permits a variable among the entries of a ``runs-on`` array. An
        entry recorded as a literal would read to every contract as a runner
        named ``${{ inputs.chosen-os }}``, a label no job can be placed on,
        while the runners the expression can actually select stay invisible to
        the contract that exists to pin them.
        """
        with pytest.raises(WorkflowShapeError, match="are expressions"):
            runner_placement(
                {"runs-on": ["self-hosted", "${{ inputs.chosen-os }}"]}, COORDINATE
            )

    def test_an_empty_label_list_is_refused(self) -> None:
        """A list naming nothing places the job nowhere a contract can assert."""
        with pytest.raises(WorkflowShapeError, match="at least one label"):
            runner_placement({"runs-on": []}, COORDINATE)

    def test_an_unmodelled_expression_is_refused(self) -> None:
        """Only the guard-and-two-literal-arms form is understood.

        A dynamic matrix expression names no label the reader can resolve, so
        recording its text as a literal would place the lane on a runner that
        does not exist.
        """
        with pytest.raises(WorkflowShapeError, match="not a guard with two"):
            runner_placement({"runs-on": "${{ fromJSON(inputs.runners) }}"}, COORDINATE)

    def test_a_runner_group_is_refused(self) -> None:
        """A mapping selects runners by group membership, not by label."""
        with pytest.raises(WorkflowShapeError, match="runs-on must be"):
            runner_placement({"runs-on": {"group": "fleet"}}, COORDINATE)

    def test_a_list_holding_a_non_string_is_refused(self) -> None:
        """A YAML scalar that is not a string cannot be a runner label."""
        with pytest.raises(WorkflowShapeError, match="runs-on must be"):
            runner_placement({"runs-on": ["ubuntu-latest", 3]}, COORDINATE)

    def test_a_missing_runs_on_is_refused(self) -> None:
        """A job that declares no placement is a shape, not a default."""
        with pytest.raises(WorkflowShapeError, match="runs-on must be"):
            runner_placement({}, COORDINATE)

    @pytest.mark.parametrize(
        "declaration",
        [
            pytest.param({"group": "fleet"}, id="runner-group"),
            pytest.param("${{ fromJSON(inputs.runners) }}", id="dynamic-matrix"),
            pytest.param(
                ["self-hosted", "${{ inputs.chosen-os }}"], id="list-variable"
            ),
        ],
    )
    def test_every_refusal_names_the_job_it_refused(self, declaration: object) -> None:
        """A refusal that does not say where it came from cannot be acted on.

        The reader walks every workflow, so a message naming only the shape
        leaves the reader hunting for which of a dozen lanes declared it.
        """
        with pytest.raises(WorkflowShapeError, match=r"ci\.yml:build-test"):
            runner_placement({"runs-on": declaration}, COORDINATE)
