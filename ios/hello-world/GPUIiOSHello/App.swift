import UIKit

@main
final class AppDelegate: UIResponder, UIApplicationDelegate {
    func application(
        _ application: UIApplication,
        didFinishLaunchingWithOptions launchOptions: [UIApplication.LaunchOptionsKey: Any]? = nil
    ) -> Bool {
        // Switch between demos by changing this call:
        //   gpui_ios_run_hello_world()              — original colored boxes
        //   gpui_ios_run_touch_demo()               — tappable boxes with feedback
        //   gpui_ios_run_text_demo()                — text rendering at various sizes
        //   gpui_ios_run_lifecycle_demo()            — window size, appearance, resize count
        //   gpui_ios_run_combined_demo()             — all features in one view
        //   gpui_ios_run_scroll_demo()               — two-finger scrollable list
        //   gpui_ios_run_text_input_demo()           — software keyboard text input
        //   gpui_ios_run_vertical_scroll_demo()      — single-finger vertical scroll (100 items)
        //   gpui_ios_run_horizontal_scroll_demo()    — single-finger horizontal card strip
        //   gpui_ios_run_pinch_demo()                — pinch-to-scale gesture
        //   gpui_ios_run_rotation_demo()             — two-finger rotation gesture
        gpui_ios_run_text_input_demo()      
        return true
    }

    func application(
        _ app: UIApplication,
        open url: URL,
        options: [UIApplication.OpenURLOptionsKey: Any] = [:]
    ) -> Bool {
        if let cString = url.absoluteString.cString(using: .utf8) {
            gpui_ios_handle_open_url(cString)
        }
        return true
    }
}
