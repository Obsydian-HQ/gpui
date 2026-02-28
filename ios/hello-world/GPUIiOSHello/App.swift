import UIKit

@main
final class AppDelegate: UIResponder, UIApplicationDelegate {
    func application(
        _ application: UIApplication,
        didFinishLaunchingWithOptions launchOptions: [UIApplication.LaunchOptionsKey: Any]? = nil
    ) -> Bool {
        // Switch between demos by changing this call:
        //   gpui_ios_run_hello_world()      — original colored boxes
        //   gpui_ios_run_touch_demo()       — tappable boxes with feedback
        //   gpui_ios_run_text_demo()        — text rendering at various sizes
        //   gpui_ios_run_lifecycle_demo()   — window size, appearance, resize count
        //   gpui_ios_run_combined_demo()    — all features in one view
        //   gpui_ios_run_scroll_demo()      — two-finger scrollable list
        //   gpui_ios_run_text_input_demo()  — software keyboard text input
        gpui_ios_run_text_input_demo()
        return true
    }
}
