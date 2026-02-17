use tuirealm::NoUserEvent;

use crate::ui_realm::tests::harness::{
    EventDriver, ExampleComponent, HarnessComponentId, HarnessMsg, MockTerminal,
    send_keys as harness_send_keys,
};
use crossterm::event::KeyCode;
use tuirealm::Application;
use tuirealm::PollStrategy;

/// Render a mounted component to a String.
///
/// This is a thin wrapper around `MockTerminal::buffer_as_string()` that handles
/// the component view rendering first.
///
/// # Arguments
/// * `app` - The mounted tui-realm Application
/// * `id` - The ComponentId of the component to render
/// * `terminal` - A pre-created MockTerminal
///
/// # Returns
/// The terminal buffer contents as a String, with newlines between rows.
pub fn render_component<I: Clone + Eq + PartialEq + std::hash::Hash, M: Clone + PartialEq>(
    app: &mut Application<I, M, NoUserEvent>,
    id: I,
    terminal: &mut MockTerminal,
) -> String {
    terminal.draw(|frame| {
        app.view(&id, frame, frame.size());
    });
    terminal.buffer_as_string()
}

/// Send keys to an application and collect the resulting messages.
///
/// This is a thin wrapper around `EventDriver` and `Application::tick()` that:
/// 1. Injects the key events into the driver
/// 2. Calls `app.tick()` with the given poll strategy
/// 3. Returns all messages produced
///
/// # Arguments
/// * `driver` - The EventDriver to inject keys into
/// * `app` - The mounted tui-realm Application
/// * `keys` - Slice of KeyCode to send sequentially
/// * `poll_count` - Number of poll iterations (default: 1)
///
/// # Returns
/// A Vec of all Msg variants produced by processing the keys.
pub fn send_key_to_component<I: Clone + Eq + PartialEq + std::hash::Hash, M: Clone + PartialEq>(
    driver: &EventDriver,
    app: &mut Application<I, M, NoUserEvent>,
    keys: &[KeyCode],
    poll_count: usize,
) -> Vec<M> {
    harness_send_keys(driver, keys);

    let mut messages = Vec::new();
    for _ in 0..poll_count {
        if let Ok(msgs) = app.tick(PollStrategy::UpTo(8)) {
            messages.extend(msgs);
        }
    }
    messages
}

/// Mount a component for testing with default setup.
///
/// This is a convenience wrapper that:
/// 1. Creates an Application with the given EventDriver listener config
/// 2. Mounts the component with empty props
/// 3. Activates the component (gives it focus)
///
/// # Arguments
/// * `driver` - The EventDriver to use for the application
/// * `component_id` - The ComponentId to mount
/// * `component` - The Component implementation to mount
///
/// # Returns
/// The configured Application with the component mounted and active.
///
/// # Type Parameters
/// * `I` - The component ID type (must match Application and ComponentId)
/// * `M` - The message type (must match Application and Component impl)
pub fn mount_component_for_test<I, M>(
    driver: &EventDriver,
    id: I,
    component: Box<dyn tuirealm::Component<M, NoUserEvent>>,
) -> Application<I, M, NoUserEvent>
where
    I: Clone + Eq + PartialEq + std::hash::Hash + 'static,
    M: Clone + PartialEq + 'static,
{
    let mut app: Application<I, M, NoUserEvent> = Application::init(driver.listener_cfg());
    app.mount(id.clone(), component, vec![])
        .expect("component should mount");
    app.active(&id).expect("component should become active");
    app
}

/// Alternative mount helper that mounts without requiring component ownership.
///
/// This is useful when you want to mount a component but keep ownership
/// or need to modify it before mounting.
///
/// # Arguments
/// * `driver` - The EventDriver to use for the application
/// * `id` - The ComponentId to mount
/// * `component` - The Component implementation (consumed on mount)
pub fn mount_component<I, M>(
    driver: &EventDriver,
    id: I,
    component: Box<dyn tuirealm::Component<M, NoUserEvent>>,
) -> Application<I, M, NoUserEvent>
where
    I: Clone + Eq + PartialEq + std::hash::Hash + 'static,
    M: Clone + PartialEq + 'static,
{
    let mut app: Application<I, M, NoUserEvent> = Application::init(driver.listener_cfg());
    app.mount(id, component, vec![])
        .expect("component should mount");
    app
}

/// Render a simple component to string for quick testing.
///
/// This is a convenience function that creates a minimal terminal and renders
/// the given component in one call.
///
/// # Arguments
/// * `app` - The mounted tui-realm Application
/// * `id` - The ComponentId of the component to render
/// * `width` - Terminal width (default: 40)
/// * `height` - Terminal height (default: 10)
///
/// # Returns
/// The terminal buffer contents as a String.
pub fn render_simple_component<
    I: Clone + Eq + PartialEq + std::hash::Hash,
    M: Clone + PartialEq,
>(
    app: &mut Application<I, M, NoUserEvent>,
    id: I,
) -> String {
    let mut terminal = MockTerminal::new(40, 10);
    render_component(app, id, &mut terminal)
}

/// Example test demonstrating helper usage.
/// Uses the harness's ExampleComponent to verify helpers work correctly.
#[test]
fn example_test() {
    use crate::ui_realm::tests::harness::{ExampleComponent, HarnessComponentId, HarnessMsg};

    let driver = EventDriver::default();
    let component = Box::new(ExampleComponent::default());
    let mut app = mount_component_for_test(&driver, HarnessComponentId::Example, component);

    // Render the component
    let output = render_simple_component(&mut app, HarnessComponentId::Example);
    assert!(
        output.contains("harness-ready"),
        "render should contain component label"
    );

    // Send keys and collect messages
    let messages = send_key_to_component(&driver, &mut app, &[KeyCode::Enter], 1);
    assert!(
        messages.contains(&HarnessMsg::Submitted(1)),
        "enter should produce Submitted message"
    );

    // Render again to see updated state
    let output = render_simple_component(&mut app, HarnessComponentId::Example);
    assert!(
        output.contains("submits: 1"),
        "after enter, submit count should be 1"
    );
}

/// Test render_component with explicit terminal dimensions.
#[test]
fn render_with_custom_dimensions() {
    let driver = EventDriver::default();
    let component = Box::new(ExampleComponent::default());
    let mut app = mount_component(&driver, HarnessComponentId::Example, component);

    let mut terminal = MockTerminal::new(80, 24);
    let output = render_component(&mut app, HarnessComponentId::Example, &mut terminal);
    assert!(!output.is_empty(), "render should produce output");
}

/// Test send_key_to_component with multiple keys.
#[test]
fn send_multiple_keys() {
    let driver = EventDriver::default();
    let component = Box::new(ExampleComponent::default());
    let mut app = mount_component_for_test(&driver, HarnessComponentId::Example, component);

    // Send multiple enter keys
    let messages = send_key_to_component(
        &driver,
        &mut app,
        &[KeyCode::Enter, KeyCode::Enter, KeyCode::Enter],
        1,
    );

    // Should have 3 Submit messages (counts 1, 2, 3)
    let submit_count = messages
        .iter()
        .filter(|m| matches!(m, HarnessMsg::Submitted(_)))
        .count();
    assert_eq!(
        submit_count, 3,
        "three enters should produce three Submit messages"
    );
}
