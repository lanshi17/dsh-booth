cmake_minimum_required(VERSION 3.28)

project(booth VERSION 0.1.0 LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 23)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)
set(CMAKE_EXPORT_COMPILE_COMMANDS ON)

option(BOOTH_WERROR "Treat compiler warnings as errors" ON)
option(BOOTH_SANITIZE "Build with AddressSanitizer and UndefinedBehaviorSanitizer" OFF)

include(FetchContent)
set(FETCHCONTENT_QUIET ON)

FetchContent_Declare(CLI11
  GIT_REPOSITORY https://github.com/CLIUtils/CLI11.git
  GIT_TAG v2.4.2
  GIT_SHALLOW TRUE
)
FetchContent_Declare(nlohmann_json
  GIT_REPOSITORY https://github.com/nlohmann/json.git
  GIT_TAG v3.11.3
  GIT_SHALLOW TRUE
)
FetchContent_Declare(Catch2
  GIT_REPOSITORY https://github.com/catchorg/Catch2.git
  GIT_TAG v3.7.1
  GIT_SHALLOW TRUE
)
FetchContent_Declare(ftxui
  GIT_REPOSITORY https://github.com/ArthurSonzogni/FTXUI.git
  GIT_TAG v7.0.3
  GIT_SHALLOW TRUE
)
set(FTXUI_BUILD_EXAMPLES OFF CACHE BOOL "" FORCE)
set(FTXUI_BUILD_DOCS OFF CACHE BOOL "" FORCE)
set(FTXUI_BUILD_TESTS OFF CACHE BOOL "" FORCE)
FetchContent_MakeAvailable(CLI11 nlohmann_json Catch2 ftxui)

find_package(Threads REQUIRED)
find_package(ICU COMPONENTS uc REQUIRED)

function(booth_warnings target)
  target_compile_options(${target} PRIVATE -Wall -Wextra -Wpedantic -Wshadow -Wnon-virtual-dtor)
  if(BOOTH_WERROR)
    target_compile_options(${target} PRIVATE -Werror)
  endif()
  if(BOOTH_SANITIZE)
    target_compile_options(${target} PRIVATE -fsanitize=address,undefined -fno-omit-frame-pointer)
    target_link_options(${target} PRIVATE -fsanitize=address,undefined)
  endif()
endfunction()

add_library(booth_contract INTERFACE)
target_include_directories(booth_contract INTERFACE ${CMAKE_CURRENT_SOURCE_DIR}/src)
target_compile_features(booth_contract INTERFACE cxx_std_23)

add_library(booth_subagent STATIC
  src/subagent/kernel.cpp
  src/subagent/acp/codec.cpp
  src/subagent/acp/messages.cpp
  src/subagent/acp/client.cpp
)
target_link_libraries(booth_subagent PUBLIC booth_contract nlohmann_json::nlohmann_json Threads::Threads)
booth_warnings(booth_subagent)

add_library(booth_supervisor STATIC
  src/supervisor/config.cpp
  src/supervisor/logs.cpp
  src/supervisor/process.cpp
  src/supervisor/supervisor.cpp
  src/supervisor/run.cpp
  src/supervisor/daemon.cpp
)
target_link_libraries(booth_supervisor PUBLIC booth_subagent nlohmann_json::nlohmann_json Threads::Threads)
booth_warnings(booth_supervisor)

add_library(booth_interaction STATIC
  src/interaction/state.cpp
  src/interaction/commands.cpp
  src/interaction/commands_ui.cpp
  src/interaction/context_gauge.cpp
  src/interaction/text_util.cpp
  src/interaction/help_scroll.cpp
  src/interaction/copy_picker.cpp
  src/interaction/link_picker.cpp
  src/interaction/mouse_mode.cpp
  src/interaction/draft.cpp
  src/interaction/file_menu.cpp
  src/interaction/history_search.cpp
  src/interaction/model_picker.cpp
  src/interaction/scroll.cpp
  src/interaction/transcript_search.cpp
  src/interaction/thought_view.cpp
  src/interaction/tool_diff.cpp
  src/interaction/tool_parse.cpp
  src/interaction/tool_view.cpp
  src/interaction/theme.cpp
  src/interaction/patch_view.cpp
  src/interaction/todo_hud.cpp
  src/interaction/worker.cpp
  src/interaction/ticker.cpp
  src/interaction/keymap.cpp
  src/interaction/history.cpp
  src/interaction/word_completion.cpp
  src/interaction/editor.cpp
  src/interaction/editor_vim.cpp
  src/interaction/vim.cpp
  src/interaction/vim_objects.cpp
  src/interaction/vim_motions.cpp
  src/interaction/emoji.cpp
  src/interaction/mention.cpp
  src/interaction/mention_menu.cpp
  src/interaction/prompt.cpp
  src/interaction/click_row.cpp
  src/interaction/mouse_zone.cpp
  src/interaction/mouse_actions.cpp
  src/interaction/open_link.cpp
  src/interaction/clipboard.cpp
  src/interaction/notify.cpp
  src/interaction/shell_command.cpp
  src/interaction/replay.cpp
  src/interaction/pickers.cpp
  src/interaction/export.cpp
  src/interaction/error_view.cpp
  src/interaction/fetch_view.cpp
  src/interaction/json_view.cpp
  src/interaction/skill_view.cpp
  src/interaction/spill_view.cpp
  src/interaction/external_editor.cpp
  src/interaction/markdown.cpp
  src/interaction/markdown_code.cpp
  src/interaction/markdown_inline.cpp
  src/interaction/markdown_table.cpp
  src/interaction/math.cpp
  src/interaction/math_block.cpp
  src/interaction/paste_buffer.cpp
  src/interaction/permission_view.cpp
  src/interaction/view.cpp
  src/interaction/tui.cpp
  src/interaction/run.cpp
)
target_link_libraries(booth_interaction PUBLIC booth_supervisor ftxui::component ftxui::dom ftxui::screen ICU::uc)
target_compile_definitions(booth_interaction PRIVATE BOOTH_VERSION="${PROJECT_VERSION}")
booth_warnings(booth_interaction)

add_executable(booth
  src/main.cpp
  src/cli.cpp
)
target_link_libraries(booth PRIVATE booth_interaction CLI11::CLI11)
target_compile_definitions(booth PRIVATE BOOTH_VERSION="${PROJECT_VERSION}")
booth_warnings(booth)

enable_testing()
add_subdirectory(tests)
