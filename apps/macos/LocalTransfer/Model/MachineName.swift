import SystemConfiguration

/// This computer's name as set in System Settings → General → Sharing,
/// e.g. "Siddhartha's MacBook Air".
enum MachineName {
    static var current: String {
        (SCDynamicStoreCopyComputerName(nil, nil) as String?) ?? "This Mac"
    }
}
