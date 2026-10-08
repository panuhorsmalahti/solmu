import XCTest

final class SolmuClientUITests: XCTestCase {
    private var app: XCUIApplication!

    private func dismissKeyboardIfVisible() {
        let dismiss = app.buttons["dismiss-keyboard"]
        if dismiss.waitForExistence(timeout: 2), dismiss.isHittable { dismiss.tap() }
    }

    override func setUpWithError() throws {
        continueAfterFailure = false
        app = XCUIApplication()
        if name.contains("BackendAddressValidation") {
            app.launchArguments = []
        } else {
            app.launchArguments = ["--uitesting", "--backend", "http://solmu.test"]
        }
        app.launch()
    }

    func testNativeConversationProfileAuditTasksAndWebhooks() throws {
        XCTAssertTrue(app.staticTexts["A little space for your next big idea."].waitForExistence(timeout: 20))
        let input = app.descendants(matching: .any).matching(identifier: "message-input").firstMatch
        XCTAssertTrue(input.waitForExistence(timeout: 5))
        input.tap()
        input.typeText("Hello from iOS")
        app.buttons["send-message"].tap()
        XCTAssertTrue(app.staticTexts["Hello from Solmu iOS"].waitForExistence(timeout: 20))
        // The response text can appear before its stream finishes. Wait for the
        // composer to become available again before submitting the next command.
        XCTAssertTrue(app.buttons["send-message"].waitForExistence(timeout: 20))
        input.tap(); input.typeText("/goal Keep iOS covered")
        app.buttons["send-message"].tap()
        XCTAssertTrue(app.alerts.staticTexts["Goal started: Keep iOS covered (goal-ios)"].waitForExistence(timeout: 10))
        app.alerts.buttons["OK"].tap()

        app.tabBars.buttons["Profile"].tap()
        let prompt = app.descendants(matching: .any).matching(identifier: "profile-prompt").firstMatch
        XCTAssertTrue(prompt.waitForExistence(timeout: 5))
        prompt.tap()
        prompt.typeText("\nBe concise.")
        dismissKeyboardIfVisible()
        app.buttons["save-profile"].tap()
        XCTAssertTrue(app.staticTexts["Edited on 2026-10-03T00:00:00Z"].waitForExistence(timeout: 10))
        if app.alerts.firstMatch.exists { app.alerts.buttons["OK"].tap() }

        app.tabBars.buttons["Audit"].tap()
        let cacheRate = app.staticTexts["audit-cache-hit-rate"]
        let cacheRateLoaded = XCTNSPredicateExpectation(predicate: NSPredicate(format: "label == %@", "40.0%"), object: cacheRate)
        XCTAssertEqual(XCTWaiter.wait(for: [cacheRateLoaded], timeout: 10), .completed)
        XCTAssertTrue(app.staticTexts["Read · completed"].exists)

        app.tabBars.buttons["Tasks"].tap()
        app.buttons["new-task"].tap()
        let taskName = app.textFields["task-name"]
        XCTAssertTrue(taskName.waitForExistence(timeout: 5))
        taskName.tap(); taskName.typeText("iOS follow-up")
        let taskPrompt = app.descendants(matching: .any).matching(identifier: "task-prompt").firstMatch
        taskPrompt.tap(); taskPrompt.typeText("Review the workspace")
        XCTAssertEqual(taskName.value as? String, "iOS follow-up")
        XCTAssertEqual(taskPrompt.value as? String, "Review the workspace")
        dismissKeyboardIfVisible()
        app.buttons["save-task"].tap()
        XCTAssertTrue(app.staticTexts["task-task-ios"].waitForExistence(timeout: 10))

        app.tabBars.buttons["More"].tap()
        app.buttons["Memories"].tap()
        XCTAssertTrue(app.staticTexts["My cat is named Miso"].waitForExistence(timeout: 10))
        XCTAssertTrue(app.staticTexts["Saved 2026-10-07T00:00:00Z"].exists)
        let memoryScreenshot = XCTAttachment(screenshot: app.screenshot())
        memoryScreenshot.name = "Solmu iOS Memories"
        memoryScreenshot.lifetime = .keepAlways
        add(memoryScreenshot)
        app.navigationBars.buttons.element(boundBy: 0).tap()
        app.tabBars.buttons["More"].tap()
        app.buttons["Webhooks"].tap()
        app.buttons["new-webhook"].tap()
        let webhookName = app.textFields["webhook-name"]
        XCTAssertTrue(webhookName.waitForExistence(timeout: 5))
        webhookName.tap(); webhookName.typeText("iOS test")
        let secret = app.secureTextFields.firstMatch
        secret.tap(); secret.typeText("ios-test-secret-123")
        dismissKeyboardIfVisible()
        app.buttons["save-webhook"].tap()
        XCTAssertTrue(app.staticTexts["webhook-hook-ios"].waitForExistence(timeout: 10))

        app.tabBars.buttons["Chat"].tap()
        XCTAssertTrue(app.staticTexts["Hello from Solmu iOS"].waitForExistence(timeout: 10))
        let screenshot = XCTAttachment(screenshot: app.screenshot())
        screenshot.name = "Solmu iOS client"
        screenshot.lifetime = .keepAlways
        add(screenshot)
    }

    func testConversationControlsAndWorkspaceStatusPages() {
        XCTAssertTrue(app.staticTexts["A little space for your next big idea."].waitForExistence(timeout: 20))
        let input = app.descendants(matching: .any).matching(identifier: "message-input").firstMatch
        input.tap(); input.typeText("Check the workspace")
        app.buttons["send-message"].tap()
        XCTAssertTrue(app.staticTexts["Hello from Solmu iOS"].waitForExistence(timeout: 20))

        app.buttons["Conversation actions"].tap()
        app.buttons["Rename"].tap()
        let title = app.textFields["rename-title"]
        XCTAssertTrue(title.waitForExistence(timeout: 5))
        title.coordinate(withNormalizedOffset: CGVector(dx: 0.95, dy: 0.5)).tap()
        title.typeText(String(repeating: XCUIKeyboardKey.delete.rawValue, count: 40)); title.typeText("Workspace review")
        XCTAssertEqual(title.value as? String, "Workspace review")
        app.buttons["Save"].tap()
        let renamedThread = app.buttons["thread-selector"]
        let titleUpdated = XCTNSPredicateExpectation(predicate: NSPredicate(format: "label == %@", "Workspace review"), object: renamedThread)
        XCTAssertEqual(XCTWaiter.wait(for: [titleUpdated], timeout: 10), .completed)

        app.buttons["Conversation actions"].tap()
        app.buttons["Model"].tap()
        let modelChoice = app.buttons.matching(NSPredicate(format: "label CONTAINS %@", "GPT 6 Sol")).firstMatch
        XCTAssertTrue(modelChoice.waitForExistence(timeout: 5))
        modelChoice.tap()

        app.buttons["Conversation actions"].tap()
        app.buttons["Context"].tap()
        XCTAssertTrue(app.staticTexts["Current context"].waitForExistence(timeout: 5))
        app.buttons["Done"].tap()

        app.tabBars.buttons["More"].tap()
        app.buttons["Skills"].tap()
        XCTAssertTrue(app.staticTexts["No skills installed in this workspace."].waitForExistence(timeout: 10))
        app.navigationBars.buttons.element(boundBy: 0).tap()
        app.buttons["MCP"].tap()
        XCTAssertTrue(app.staticTexts["No MCP servers configured in this workspace."].waitForExistence(timeout: 10))
        app.navigationBars.buttons.element(boundBy: 0).tap()
        app.buttons["Plugins"].tap()
        XCTAssertTrue(app.staticTexts["No plugins installed in this workspace."].waitForExistence(timeout: 10))
    }

    func testBackendAddressValidationIsNativeAndActionable() {
        XCTAssertTrue(app.navigationBars["Welcome"].waitForExistence(timeout: 10))
        let address = app.textFields["backend-address"]
        address.tap()
        address.typeText(String(repeating: XCUIKeyboardKey.delete.rawValue, count: 40))
        address.typeText("invalid address")
        app.buttons["connect-backend"].tap()
        XCTAssertTrue(app.staticTexts["Enter a valid http:// or https:// backend address."].exists)
    }
}
