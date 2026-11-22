add_executable(fake_kernel fake_kernel.cpp)
target_link_libraries(fake_kernel PRIVATE nlohmann_json::nlohmann_json)
booth_warnings(fake_kernel)

add_executable(booth_tests contract_test.cpp acp_test.cpp supervisor_test.cpp interaction_test.cpp scroll_test.cpp keymap_test.cpp editor_test.cpp replay_test.cpp clipboard_test.cpp)
target_link_libraries(booth_tests PRIVATE booth_interaction Catch2::Catch2WithMain)
target_compile_definitions(booth_tests PRIVATE FAKE_KERNEL_PATH="$<TARGET_FILE:fake_kernel>")
add_dependencies(booth_tests fake_kernel)
booth_warnings(booth_tests)

include(Catch)
catch_discover_tests(booth_tests)

add_test(NAME pty_smoke
  COMMAND ${CMAKE_CURRENT_SOURCE_DIR}/pty_smoke.sh $<TARGET_FILE:booth> $<TARGET_FILE:fake_kernel>
)

add_test(NAME daemon_smoke
  COMMAND ${CMAKE_CURRENT_SOURCE_DIR}/daemon_smoke.sh $<TARGET_FILE:booth> $<TARGET_FILE:fake_kernel>
)

add_test(NAME pty_daemon_smoke
  COMMAND ${CMAKE_CURRENT_SOURCE_DIR}/pty_daemon_smoke.sh $<TARGET_FILE:booth> $<TARGET_FILE:fake_kernel>
)

add_test(NAME pty_permission
  COMMAND ${CMAKE_CURRENT_SOURCE_DIR}/pty_permission.sh $<TARGET_FILE:booth> $<TARGET_FILE:fake_kernel>
)
add_test(NAME pty_cancel
  COMMAND ${CMAKE_CURRENT_SOURCE_DIR}/pty_cancel.sh $<TARGET_FILE:booth> $<TARGET_FILE:fake_kernel>
)

add_test(NAME pty_sessions
  COMMAND ${CMAKE_CURRENT_SOURCE_DIR}/pty_sessions.sh $<TARGET_FILE:booth> $<TARGET_FILE:fake_kernel>
)

add_test(NAME pty_permission_always
  COMMAND ${CMAKE_CURRENT_SOURCE_DIR}/pty_permission_always.sh $<TARGET_FILE:booth> $<TARGET_FILE:fake_kernel>
)

add_test(NAME pty_sessions_picker
  COMMAND ${CMAKE_CURRENT_SOURCE_DIR}/pty_sessions_picker.sh $<TARGET_FILE:booth> $<TARGET_FILE:fake_kernel>
)

add_test(NAME pty_commands
  COMMAND ${CMAKE_CURRENT_SOURCE_DIR}/pty_commands.sh $<TARGET_FILE:booth> $<TARGET_FILE:fake_kernel>
)

add_test(NAME pty_sessions_pages
  COMMAND ${CMAKE_CURRENT_SOURCE_DIR}/pty_sessions_pages.sh $<TARGET_FILE:booth> $<TARGET_FILE:fake_kernel>
)

add_test(NAME pty_markdown
  COMMAND ${CMAKE_CURRENT_SOURCE_DIR}/pty_markdown.sh $<TARGET_FILE:booth> $<TARGET_FILE:fake_kernel>
)

add_test(NAME pty_export
  COMMAND ${CMAKE_CURRENT_SOURCE_DIR}/pty_export.sh $<TARGET_FILE:booth> $<TARGET_FILE:fake_kernel>
)

add_test(NAME pty_editor
  COMMAND ${CMAKE_CURRENT_SOURCE_DIR}/pty_editor.sh $<TARGET_FILE:booth> $<TARGET_FILE:fake_kernel>
)

add_test(NAME pty_editor_binding
  COMMAND ${CMAKE_CURRENT_SOURCE_DIR}/pty_editor_binding.sh $<TARGET_FILE:booth> $<TARGET_FILE:fake_kernel>
)

add_test(NAME pty_scroll
  COMMAND ${CMAKE_CURRENT_SOURCE_DIR}/pty_scroll.sh $<TARGET_FILE:booth> $<TARGET_FILE:fake_kernel>
)

add_test(NAME pty_reasoning
  COMMAND ${CMAKE_CURRENT_SOURCE_DIR}/pty_reasoning.sh $<TARGET_FILE:booth> $<TARGET_FILE:fake_kernel>
)

add_test(NAME pty_wheel
  COMMAND ${CMAKE_CURRENT_SOURCE_DIR}/pty_wheel.sh $<TARGET_FILE:booth> $<TARGET_FILE:fake_kernel>
)

add_test(NAME pty_busy_commands
  COMMAND ${CMAKE_CURRENT_SOURCE_DIR}/pty_busy_commands.sh $<TARGET_FILE:booth> $<TARGET_FILE:fake_kernel>
)

add_test(NAME pty_queue
  COMMAND ${CMAKE_CURRENT_SOURCE_DIR}/pty_queue.sh $<TARGET_FILE:booth> $<TARGET_FILE:fake_kernel>
)

add_test(NAME pty_model_wheel
  COMMAND ${CMAKE_CURRENT_SOURCE_DIR}/pty_model_wheel.sh $<TARGET_FILE:booth> $<TARGET_FILE:fake_kernel>
)

add_test(NAME pty_cancel_permission
  COMMAND ${CMAKE_CURRENT_SOURCE_DIR}/pty_cancel_permission.sh $<TARGET_FILE:booth> $<TARGET_FILE:fake_kernel>
)

add_test(NAME pty_clear
  COMMAND ${CMAKE_CURRENT_SOURCE_DIR}/pty_clear.sh $<TARGET_FILE:booth> $<TARGET_FILE:fake_kernel>
)

add_test(NAME pty_retry
  COMMAND ${CMAKE_CURRENT_SOURCE_DIR}/pty_retry.sh $<TARGET_FILE:booth> $<TARGET_FILE:fake_kernel>
)
