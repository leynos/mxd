//! Consumer regressions for result aliases and fixture binding during the BDD upgrade.

use rstest::{fixture, rstest};
use rstest_bdd::{StepContext, StepError, StepExecution, StepKeyword, StepText, find_step};
use rstest_bdd_macros::{then, when};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Number(i32);

type ConsumerResult<T> = Result<T, String>;

#[fixture]
fn number() -> Number {
    let initial_value = 1;
    Number(initial_value)
}

#[when("migration replaces the number with {value:i32}")]
fn replace_number(#[from(number)] _number: &Number, value: i32) -> ConsumerResult<Number> {
    if value < 0 {
        Err("negative replacement rejected".to_owned())
    } else {
        Ok(Number(value))
    }
}

#[then("migration resolves an implicit unused fixture")]
fn implicit_fixture(_number: &Number) {}

#[then("migration removes exactly one underscore")]
fn one_underscore(__number: &Number) {}

#[then("migration preserves an explicit underscore key")]
fn explicit_fixture(#[from(_number)] selected: &Number) {
    assert_eq!(*selected, Number(1));
}

#[rstest]
fn aliased_step_error_propagates(number: Number) {
    let mut context = StepContext::default();
    context.insert("number", &number);
    let text = "migration replaces the number with -1";
    let step =
        find_step(StepKeyword::When, StepText::from(text)).expect("replacement step registered");
    let Err(StepError::ExecutionError { message, .. }) = step(&mut context, text, None, None)
    else {
        panic!("an aliased Err must fail the step rather than become a payload");
    };
    assert_eq!(message, "negative replacement rejected");
}

#[rstest]
fn aliased_success_overrides_the_matching_fixture(number: Number) {
    let mut context = StepContext::default();
    context.insert("number", &number);
    let text = "migration replaces the number with 7";
    let step =
        find_step(StepKeyword::When, StepText::from(text)).expect("replacement step registered");
    let outcome = step(&mut context, text, None, None).expect("positive replacement succeeds");
    let StepExecution::Continue { value: Some(value) } = outcome else {
        panic!("the aliased Ok must supply its inner value");
    };
    assert!(context.insert_value(value).is_inserted());
    let actual = context
        .borrow_ref::<Number>("number")
        .expect("number fixture remains available");
    assert_eq!(*actual.value(), Number(7));
    assert_eq!(number, Number(1), "the original fixture is unchanged");
}

#[rstest]
#[case("migration resolves an implicit unused fixture", "number")]
#[case("migration removes exactly one underscore", "_number")]
#[case("migration preserves an explicit underscore key", "_number")]
fn fixture_keys_remain_deliberate(number: Number, #[case] text: &str, #[case] key: &'static str) {
    let mut context = StepContext::default();
    context.insert(key, &number);
    let step = find_step(StepKeyword::Then, StepText::from(text)).expect("fixture step registered");
    let outcome = step(&mut context, text, None, None).expect("fixture key resolves");
    assert!(matches!(outcome, StepExecution::Continue { value: None }));
}

#[when("migration work suspends before returning a replacement")]
async fn yielding_replacement() -> ConsumerResult<Number> {
    let task = tokio::spawn(async {
        tokio::task::yield_now().await;
        Number(7)
    });
    task.await.map_err(|error| error.to_string())
}

#[then("migration observes replacement {expected:i32}")]
fn replacement_visible(number: &Number, expected: i32) {
    assert_eq!(number.0, expected);
}

#[rstest_bdd_macros::scenario(path = "tests/features/rstest_bdd_migration.feature")]
#[tokio::test(flavor = "current_thread")]
async fn yielding_steps_override_only_their_scenario_fixture(number: Number) {
    assert_eq!(
        number,
        Number(1),
        "step overrides leave the original fixture intact"
    );
}
