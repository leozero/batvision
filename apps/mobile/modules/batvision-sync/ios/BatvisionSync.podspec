Pod::Spec.new do |s|
  s.name           = 'BatvisionSync'
  s.version        = '1.0.0'
  s.summary        = 'BatVision download engine for iOS'
  s.description    = 'Background downloads, offline library and progress for the BatVision app (D86, D87)'
  s.license        = 'GPL-3.0-only'
  s.author         = 'BatVision'
  s.homepage       = 'https://github.com/leozero/batvision'
  s.platforms      = { :ios => '16.4' }
  s.source         = { git: 'https://github.com/leozero/batvision.git' }
  s.static_framework = true

  s.dependency 'ExpoModulesCore'

  # Package.swift and CoreTests/ only serve `swift test` on the Mac.
  s.source_files = '*.swift', 'Core/*.swift'
  s.exclude_files = 'Package.swift'
  s.frameworks = 'BackgroundTasks', 'Network', 'VideoToolbox', 'AVFoundation'
  s.pod_target_xcconfig = {
    'DEFINES_MODULE' => 'YES',
    'SWIFT_COMPILATION_MODE' => 'wholemodule'
  }
end
