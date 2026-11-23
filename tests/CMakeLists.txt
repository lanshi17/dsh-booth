add_executable(fake_kernel fake_kernel.cpp)
target_link_libraries(fake_kernel PRIVATE nlohmann_json::nlohmann_json)
booth_warnings(fake_kernel)

add_executable(booth_tests contract_test.cpp click_row_test.cpp mouse_zone_test.cpp mouse_picker_test.cpp mouse_actions_test.cpp acp_test.cpp supervisor_test.cpp interaction_test.cpp scroll_test.cpp slash_test.cpp commands_ui_test.cpp copy_picker_test.cpp picker_filter_test.cpp draft_test.cpp file_menu_test.cpp tool_diff_test.cpp theme_test.cpp emoji_test.cpp mention_test.cpp mention_menu_test.cpp history_search_test.cpp keymap_test.cpp editor_test.cpp vim_test.cpp replay_test.cpp clipboard_test.cpp notify_test.cpp word_completion_test.cpp link_picker_test.cpp shell_command_test.cpp)
target_link_libraries(booth_tests PRIVATE booth_interaction Catch2::Catch2WithMain)
target_compile_definitions(booth_tests PRIVATE FAKE_KERNEL_PATH="$<TARGET_FILE:fake_kernel>")
add_dependencies(booth_tests fake_kernel)
booth_warnings(booth_tests)

include(Catch)
catch_discover_tests(booth_tests)

add_test(NAME pty_smoke
  COMMAND ${CMAKE_CURRENT_SOURCE_DIR}/pty_smoke.sh $<TARGET_FILE:booth> $<TARGET_FILE:fake_kernel>
)

add_test(NAME pty_shell_command
  COMMAND ${CMAKE_CURRENT_SOURCE_DIR}/pty_shell_command.sh $<TARGET_FILE:booth> $<TARGET_FILE:fake_kernel>
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

add_test(NAME pty_notify
  COMMAND ${CMAKE_CURRENT_SOURCE_DIR}/pty_notify.sh $<TARGET_FILE:booth> $<TARGET_FILE:fake_kernel>
)

add_test(NAME pty_links
  COMMAND ${CMAKE_CURRENT_SOURCE_DIR}/pty_links.sh $<TARGET_FILE:booth> $<TARGET_FILE:fake_kernel>
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

add_test(NAME pty_slash_fuzzy
  COMMAND ${CMAKE_CURRENT_SOURCE_DIR}/pty_slash_fuzzy.sh $<TARGET_FILE:booth> $<TARGET_FILE:fake_kernel>
)

add_test(NAME pty_slash_escape
  COMMAND ${CMAKE_CURRENT_SOURCE_DIR}/pty_slash_escape.sh $<TARGET_FILE:booth> $<TARGET_FILE:fake_kernel>
)

add_test(NAME pty_slash_pages
  COMMAND ${CMAKE_CURRENT_SOURCE_DIR}/pty_slash_pages.sh $<TARGET_FILE:booth> $<TARGET_FILE:fake_kernel>
)

add_test(NAME pty_slash_menu
  COMMAND ${CMAKE_CURRENT_SOURCE_DIR}/pty_slash_menu.sh $<TARGET_FILE:booth> $<TARGET_FILE:fake_kernel>
)

add_test(NAME pty_new
  COMMAND ${CMAKE_CURRENT_SOURCE_DIR}/pty_new.sh $<TARGET_FILE:booth> $<TARGET_FILE:fake_kernel>
)

add_test(NAME pty_effort
  COMMAND ${CMAKE_CURRENT_SOURCE_DIR}/pty_effort.sh $<TARGET_FILE:booth> $<TARGET_FILE:fake_kernel>
)

add_test(NAME pty_queue_command
  COMMAND ${CMAKE_CURRENT_SOURCE_DIR}/pty_queue_command.sh $<TARGET_FILE:booth> $<TARGET_FILE:fake_kernel>
)

add_test(NAME pty_stats
  COMMAND ${CMAKE_CURRENT_SOURCE_DIR}/pty_stats.sh $<TARGET_FILE:booth> $<TARGET_FILE:fake_kernel>
)

add_test(NAME pty_context
  COMMAND ${CMAKE_CURRENT_SOURCE_DIR}/pty_context.sh $<TARGET_FILE:booth> $<TARGET_FILE:fake_kernel>
)

add_test(NAME pty_usage
  COMMAND ${CMAKE_CURRENT_SOURCE_DIR}/pty_usage.sh $<TARGET_FILE:booth> $<TARGET_FILE:fake_kernel>
)

add_test(NAME pty_theme
  COMMAND ${CMAKE_CURRENT_SOURCE_DIR}/pty_theme.sh $<TARGET_FILE:booth> $<TARGET_FILE:fake_kernel>
)

add_test(NAME pty_vim
  COMMAND ${CMAKE_CURRENT_SOURCE_DIR}/pty_vim.sh $<TARGET_FILE:booth> $<TARGET_FILE:fake_kernel>
)

add_test(NAME pty_image
  COMMAND ${CMAKE_CURRENT_SOURCE_DIR}/pty_image.sh $<TARGET_FILE:booth> $<TARGET_FILE:fake_kernel>
)

add_test(NAME pty_vim_visual
  COMMAND ${CMAKE_CURRENT_SOURCE_DIR}/pty_vim_visual.sh $<TARGET_FILE:booth> $<TARGET_FILE:fake_kernel>
)

add_test(NAME pty_mention
  COMMAND ${CMAKE_CURRENT_SOURCE_DIR}/pty_mention.sh $<TARGET_FILE:booth> $<TARGET_FILE:fake_kernel>
)


add_test(NAME pty_find
  COMMAND ${CMAKE_CURRENT_SOURCE_DIR}/pty_find.sh $<TARGET_FILE:booth> $<TARGET_FILE:fake_kernel>
)

add_test(NAME pty_draft
  COMMAND ${CMAKE_CURRENT_SOURCE_DIR}/pty_draft.sh $<TARGET_FILE:booth> $<TARGET_FILE:fake_kernel>
)

add_test(NAME pty_composer_scroll
  COMMAND ${CMAKE_CURRENT_SOURCE_DIR}/pty_composer_scroll.sh $<TARGET_FILE:booth> $<TARGET_FILE:fake_kernel>
)

add_test(NAME pty_dequeue
  COMMAND ${CMAKE_CURRENT_SOURCE_DIR}/pty_dequeue.sh $<TARGET_FILE:booth> $<TARGET_FILE:fake_kernel>
)

add_test(NAME pty_history_search
  COMMAND ${CMAKE_CURRENT_SOURCE_DIR}/pty_history_search.sh $<TARGET_FILE:booth> $<TARGET_FILE:fake_kernel>
)

add_test(NAME pty_tool_expand
  COMMAND ${CMAKE_CURRENT_SOURCE_DIR}/pty_tool_expand.sh $<TARGET_FILE:booth> $<TARGET_FILE:fake_kernel>
)

add_test(NAME pty_tool_diff
  COMMAND ${CMAKE_CURRENT_SOURCE_DIR}/pty_tool_diff.sh $<TARGET_FILE:booth> $<TARGET_FILE:fake_kernel>
)

add_test(NAME pty_thinking
  COMMAND ${CMAKE_CURRENT_SOURCE_DIR}/pty_thinking.sh $<TARGET_FILE:booth> $<TARGET_FILE:fake_kernel>
)

add_test(NAME pty_attach
  COMMAND ${CMAKE_CURRENT_SOURCE_DIR}/pty_attach.sh $<TARGET_FILE:booth> $<TARGET_FILE:fake_kernel>
)

add_test(NAME pty_copy_picker
  COMMAND ${CMAKE_CURRENT_SOURCE_DIR}/pty_copy_picker.sh $<TARGET_FILE:booth> $<TARGET_FILE:fake_kernel>
)

add_test(NAME pty_model_filter
  COMMAND ${CMAKE_CURRENT_SOURCE_DIR}/pty_model_filter.sh $<TARGET_FILE:booth> $<TARGET_FILE:fake_kernel>
)

add_test(NAME pty_at_file
  COMMAND ${CMAKE_CURRENT_SOURCE_DIR}/pty_at_file.sh $<TARGET_FILE:booth> $<TARGET_FILE:fake_kernel>
)

add_test(NAME pty_session_filter
  COMMAND ${CMAKE_CURRENT_SOURCE_DIR}/pty_session_filter.sh $<TARGET_FILE:booth> $<TARGET_FILE:fake_kernel>
)

add_test(NAME pty_session_acp
  COMMAND ${CMAKE_CURRENT_SOURCE_DIR}/pty_session_acp.sh $<TARGET_FILE:booth> $<TARGET_FILE:fake_kernel>
)

add_test(NAME pty_cancel_command
  COMMAND ${CMAKE_CURRENT_SOURCE_DIR}/pty_cancel_command.sh $<TARGET_FILE:booth> $<TARGET_FILE:fake_kernel>
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

add_test(NAME pty_word_completion
  COMMAND ${CMAKE_CURRENT_SOURCE_DIR}/pty_word_completion.sh $<TARGET_FILE:booth> $<TARGET_FILE:fake_kernel>
)
