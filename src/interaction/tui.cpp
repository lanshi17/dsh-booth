#include "interaction/tui.hpp"
#include "interaction/tui_internal.hpp"

#include <ftxui/component/component.hpp>
#include <ftxui/component/component_base.hpp>
#include <ftxui/component/component_options.hpp>
#include <ftxui/component/mouse.hpp>
#include <ftxui/component/screen_interactive.hpp>
#include <ftxui/dom/elements.hpp>

#include <nlohmann/json.hpp>

#include <sys/ioctl.h>
#include <unistd.h>

#include <cctype>
#include <atomic>
#include <cstdlib>
#include <chrono>
#include <condition_variable>
#include <cstddef>
#include <deque>
#include <filesystem>
#include <fstream>
#include <functional>
#include <iostream>
#include <memory>
#include <mutex>
#include <optional>
#include <string>
#include <thread>
#include <utility>
#include <vector>

#include "interaction/keymap.hpp"
#include "interaction/editor.hpp"
#include "interaction/export.hpp"
#include "interaction/external_editor.hpp"
#include "interaction/clipboard.hpp"
#include "interaction/history.hpp"
#include "interaction/prompt.hpp"
#include "interaction/scroll.hpp"
#include "interaction/commands_ui.hpp"
#include "interaction/state.hpp"
#include "interaction/ticker.hpp"
#include "interaction/worker.hpp"
#include "interaction/view.hpp"
#include "subagent/acp/messages.hpp"
#include "subagent/kernel.hpp"
#include "supervisor/daemon.hpp"
#include "supervisor/supervisor.hpp"

namespace booth {
namespace {

using ftxui::Component;
using ftxui::ComponentBase;
using ftxui::Element;
using ftxui::Elements;
using ftxui::ScreenInteractive;

const char* const kSpinner[] = {"⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧"};

class Root : public ComponentBase {
 public:
  Root(ScreenInteractive& screen, Kernel& supervisor, AppState& state, Inbox& inbox,
       Worker& worker, std::string cwd, std::filesystem::path history_file,
       std::filesystem::path sessions_dir, KeyBindings keys)
      : screen_(screen),
        supervisor_(supervisor),
        state_(state),
        inbox_(inbox),
        worker_(worker),
        editor_(state.composer),
        cwd_(std::move(cwd)),
        history_file_(std::move(history_file)),
        sessions_dir_(std::move(sessions_dir)),
        keys_(keys) {
    history_ = booth::load_history(history_file_);
    git_ = booth::read_git(cwd_);
    ticker_ = booth::start_ticker(screen_, animate_, alive_);
  }
  void shutdown() {
    cancel();
    animate_ = false;
    alive_ = false;
    if (ticker_.joinable()) ticker_.join();
    worker_.stop();
  }
  Element OnRender() override {
    if (state_.permission) {
      return booth::permission_view(*state_.permission) |
             ftxui::size(ftxui::WIDTH, ftxui::EQUAL, 62) | ftxui::borderRounded | ftxui::center;
    }
    if (state_.help) {
      return booth::help_view() | ftxui::size(ftxui::WIDTH, ftxui::EQUAL, 62) |
             ftxui::borderRounded | ftxui::center;
    }
    return main_view();
  }
  bool OnEvent(ftxui::Event event) override {
    if (event == ftxui::Event::Custom) {
      drain();
      return true;
    }
    if (matches_chord(keys_.cancel, event)) {
      cancel();
      return true;
    }
    if (state_.help) {
      if (event == ftxui::Event::Escape || event == ftxui::Event::Return ||
          event == ftxui::Event::Character('q')) {
        state_.help = false;
      }
      return true;
    }
    if (state_.permission) {
      if (event == ftxui::Event::Character('y') || event == ftxui::Event::Return) {
        answer("allow_once");
      } else if (event == ftxui::Event::Character('a')) {
        answer("allow_always");
      } else if (event == ftxui::Event::Character('n') || event == ftxui::Event::Escape) {
        answer("reject_once");
      } else if (event == ftxui::Event::Character('d')) {
        answer("reject_always");
      }
      return true;
    }
    if (event.input() == "\x1b[Z") {
      if (state_.reasoning.choices.empty()) {
        state_.notice = "no reasoning options";
        return true;
      }
      std::size_t current = 0;
      for (std::size_t index = 0; index < state_.reasoning.choices.size(); ++index) {
        if (state_.reasoning.choices[index].value == state_.reasoning.current) current = index;
      }
      const auto choice = state_.reasoning.choices[(current + 1) % state_.reasoning.choices.size()];
      worker_.post([this, ctx = context(), id = state_.reasoning.id, choice]() mutable {
        booth::set_reasoning(ctx, id, choice);
      });
      state_.notice = "reasoning " + choice.name;
      return true;
    }
    if (sessions_open_) {
      if (event == ftxui::Event::Escape) {
        sessions_open_ = false;
      } else if (event == ftxui::Event::ArrowUp && !state_.history.empty()) {
        sessions_index_ = (sessions_index_ + state_.history.size() - 1) % state_.history.size();
      } else if (event == ftxui::Event::ArrowDown && !state_.history.empty()) {
        sessions_index_ = (sessions_index_ + 1) % state_.history.size();
      } else if (event.is_mouse() && !state_.history.empty() &&
                 event.mouse().button == ftxui::Mouse::WheelUp) {
        sessions_index_ = (sessions_index_ + state_.history.size() - 1) % state_.history.size();
      } else if (event.is_mouse() && !state_.history.empty() &&
                 event.mouse().button == ftxui::Mouse::WheelDown) {
        sessions_index_ = (sessions_index_ + 1) % state_.history.size();
      } else if (event == ftxui::Event::Character('r')) {
        sessions_loading_ = true;
        worker_.post([this, ctx = context()]() mutable { booth::list_sessions(ctx); });
      } else if (event == ftxui::Event::Character('c')) {
        sessions_open_ = false;
        state_.notice = "closing…";
        worker_.post([this, ctx = context()]() mutable { booth::close_session(ctx); });
      } else if (event == ftxui::Event::Return && !sessions_loading_ &&
                 !state_.history.empty()) {
        const auto selected = state_.history[sessions_index_];
        sessions_open_ = false;
        state_.notice = "resuming…";
        worker_.post([this, ctx = context(), selected]() mutable {
          booth::resume_session(ctx, selected);
        });
      }
      return true;
    }
    if (picker_open_) {
      if (event == ftxui::Event::Escape) {
        picker_open_ = false;
      } else if (event == ftxui::Event::ArrowUp) {
        picker_index_ = (picker_index_ + state_.models.size() - 1) % state_.models.size();
      } else if (event == ftxui::Event::ArrowDown) {
        picker_index_ = (picker_index_ + 1) % state_.models.size();
      } else if (event.is_mouse() && event.mouse().button == ftxui::Mouse::WheelUp) {
        picker_index_ = (picker_index_ + state_.models.size() - 1) % state_.models.size();
      } else if (event.is_mouse() && event.mouse().button == ftxui::Mouse::WheelDown) {
        picker_index_ = (picker_index_ + 1) % state_.models.size();
      } else if (event == ftxui::Event::Return) {
        apply_model(state_.models[picker_index_]);
        picker_open_ = false;
      }
      return true;
    }
    slash_.sync(state_.composer);
    if (slash_.visible(state_.composer)) {
      const auto action = slash_.key(event, state_);
      if (action != booth::SlashAction::None) {
        editor_.SetContent(state_.composer);
        if (action == booth::SlashAction::Submit) submit();
        return true;
      }
    }
    if (event.is_mouse()) {
      const auto& mouse = event.mouse();
      if (mouse.button == ftxui::Mouse::WheelUp) { scroll_.up(state_.entries.size(), 3); return true; }
      if (mouse.button == ftxui::Mouse::WheelDown) { scroll_.down(3); return true; }
    }
    if (event == ftxui::Event::PageUp) {
      scroll_.page_up(state_.entries.size());
      return true;
    }
    if (event == ftxui::Event::PageDown) {
      scroll_.page_down();
      return true;
    }
    if (matches_chord(keys_.editor, event)) {
      if (booth::edit_in_screen(screen_, state_.composer, state_.notice)) {
        editor_.SetContent(state_.composer);
      }
      return true;
    }
    if (matches_chord(keys_.quit, event) && state_.composer.empty()) {
      screen_.Exit();
      return true;
    }
    if (event == ftxui::Event::ArrowUp && state_.composer.empty() && !history_.empty()) {
      if (history_pos_ < history_.size()) ++history_pos_;
      state_.composer = history_[history_.size() - history_pos_];
      editor_.SetContent(state_.composer);
      return true;
    }
    if (event == ftxui::Event::ArrowDown && history_pos_ > 0) {
      --history_pos_;
      state_.composer = history_pos_ == 0 ? std::string{} : history_[history_.size() - history_pos_];
      editor_.SetContent(state_.composer);
      return true;
    }
    if (event == ftxui::Event::Return) {
      submit();
      return true;
    }
    if (editor_.HandleEvent(event)) return true;
    return false;
  }
 private:
  Element main_view() {
    Elements lines;
    const auto window = scroll_.window(state_.entries.size());
    for (std::size_t index = window.begin; index < window.end; ++index) {
      lines.push_back(booth::entry_element(state_.entries[index]));
    }
    if (window.scroll == 0 && !state_.streaming.empty()) {
      lines.push_back(booth::markdown_view(state_.streaming, false));
    }
    if (state_.busy && window.scroll == 0) {
      const auto elapsed = std::chrono::steady_clock::now() - busy_since_;
      const auto seconds =
          std::chrono::duration_cast<std::chrono::seconds>(elapsed).count();
      const auto ms = std::chrono::duration_cast<std::chrono::milliseconds>(elapsed).count();
      lines.push_back(ftxui::text(std::string("  ") + kSpinner[(ms / 120) % 8] + " " +
                                  std::to_string(seconds) + "s Working…") |
                      ftxui::color(ftxui::Color::RGB(228, 228, 228)));
    }
    Elements view;
    view.push_back(ftxui::vbox(std::move(lines)) | ftxui::flex);
    if (!state_.notice.empty()) {
      view.push_back(ftxui::paragraph(state_.notice) | ftxui::color(ftxui::Color::RGB(255, 95, 95)));
    }
    if (window.scroll > 0) {
      view.push_back(ftxui::text("  ↑ " + std::to_string(window.scroll) +
                                 " entries back · PageDown to follow") |
                     ftxui::color(ftxui::Color::RGB(150, 150, 150)));
    }
    if (!queued_.empty()) {
      view.push_back(ftxui::text("  " + std::to_string(queued_.size()) + " queued") |
                     ftxui::color(ftxui::Color::RGB(150, 150, 150)));
    }
    if (sessions_open_) {
      view.push_back(booth::session_picker_view(state_.history, sessions_index_, sessions_loading_));
    }
    if (picker_open_ && !state_.models.empty()) {
      view.push_back(booth::model_picker_view(state_.models, picker_index_, state_.model));
    }
    view.push_back(ftxui::text(" "));
    long busy_seconds = -1;
    std::string spinner;
    if (state_.busy && window.scroll == 0) {
      const auto elapsed = std::chrono::steady_clock::now() - busy_since_;
      const auto ms = std::chrono::duration_cast<std::chrono::milliseconds>(elapsed).count();
      busy_seconds = static_cast<long>(ms / 1000);
      spinner = kSpinner[(ms / 120) % 8];
    }
    const std::string& title = state_.title.empty() ? session_title_ : state_.title;
    view.push_back(booth::composer_view(state_, cwd_, git_, title, busy_seconds, spinner,
                                        editor_.Lines(), editor_.CursorLine(),
                                        editor_.CursorColumn(), state_.reasoning.current_name));
    slash_.sync(state_.composer);
    if (slash_.visible(state_.composer)) {
      view.push_back(booth::command_menu_view(state_.composer, slash_.index()));
    }
    return ftxui::vbox(std::move(view));
  }
  void drain() {
    std::deque<Event> events;
    {
      std::lock_guard<std::mutex> lock(inbox_.mutex);
      events.swap(inbox_.events);
    }
    const std::size_t before = state_.entries.size();
    bool switched = false;
    for (auto& event : events) {
      if (std::holds_alternative<SessionSwitched>(event.value)) switched = true;
      const bool turn_done = std::holds_alternative<StreamEnd>(event.value);
      const bool listed = std::holds_alternative<SessionsListed>(event.value);
      const bool closed = std::holds_alternative<SessionClosed>(event.value);
      apply(state_, event);
      if (listed) {
        sessions_loading_ = false;
        if (state_.history.empty()) {
          sessions_index_ = 0;
        } else if (sessions_index_ >= state_.history.size()) {
          sessions_index_ = state_.history.size() - 1;
        }
      }
      if (std::holds_alternative<Problem>(event.value)) new_after_close_ = false;
      if (closed) {
        session_title_.clear();
        queued_.clear();
        if (new_after_close_) {
          state_.notice = "new session";
          new_after_close_ = false;
        }
      }
      if (turn_done) {
        animate_ = false;
        git_ = booth::read_git(cwd_);
      }
    }
    if (switched) {
      scroll_.follow();
      queued_.clear();
    } else {
      scroll_.resized(before, state_.entries.size());
    }
    if (!state_.busy && !state_.permission && !queued_.empty()) {
      auto next = std::move(queued_.front());
      queued_.pop_front();
      send_prompt(std::move(next));
    }
  }
  void publish(Event event) {
    {
      std::lock_guard<std::mutex> lock(inbox_.mutex);
      inbox_.events.push_back(std::move(event));
    }
    screen_.PostEvent(ftxui::Event::Custom);
  }
  void submit() {
    std::string text = state_.composer;
    const auto first = text.find_first_not_of(" \t\r\n");
    if (first == std::string::npos) return;
    const auto last = text.find_last_not_of(" \t\r\n");
    text = text.substr(first, last - first + 1);
    const bool command = text.front() == '/';
    if (state_.permission) return;
    if (state_.busy) {
      if (command && booth::local_command(text)) {
        state_.composer.clear();
        editor_.Reset();
        handle_command(text);
      } else if (!command) {
        state_.composer.clear();
        editor_.Reset();
        queued_.push_back(std::move(text));
        state_.notice = std::to_string(queued_.size()) + " queued";
      } else {
        state_.notice = "busy: local commands and /queue still work";
      }
      return;
    }
    state_.composer.clear();
    editor_.Reset();
    if (command) {
      handle_command(text);
      return;
    }
    send_prompt(std::move(text));
  }
  void send_prompt(std::string text) {
    state_.notice.clear();
    state_.busy = true;
    animate_ = true;
    if (session_title_.empty()) session_title_ = text;
    busy_since_ = std::chrono::steady_clock::now();
    TranscriptEntry echo;
    echo.kind = EntryKind::User;
    echo.text = text;
    publish(Event{std::move(echo)});
    scroll_.follow();
    history_.push_back(text);
    history_pos_ = 0;
    booth::append_history(history_file_, text);
    worker_.post([this, text = std::move(text)] { run_prompt(text); });
  }
  void handle_command(std::string line) {
    line = booth::normalize_command(std::move(line));
    if (booth::handle_local_command(state_, scroll_, queued_.size(), line)) return;
    if (line == "/queue" || line.rfind("/queue ", 0) == 0) {
      booth::handle_queue_command(state_, queued_, line);
      return;
    }
    if (line == "/effort" || line.rfind("/effort ", 0) == 0) {
      booth::handle_effort_command(state_, line, [this](const std::string& id, const ModelChoice& picked) {
        worker_.post([this, ctx = context(), id, choice = picked]() mutable { booth::set_reasoning(ctx, id, choice); });
      });
      return;
    }
    if (line == "/new") {
      if (booth::start_new_session(state_, scroll_, new_after_close_))
        worker_.post([this, ctx = context()]() mutable { booth::close_session(ctx); });
      return;
    }
    const auto argument = [&](std::string_view prefix) -> std::optional<std::string> {
      if (line.rfind(prefix, 0) != 0) return std::nullopt;
      std::string rest = line.substr(prefix.size());
      const auto first = rest.find_first_not_of(" \t");
      if (first == std::string::npos) return std::string{};
      rest = rest.substr(first);
      const auto last = rest.find_last_not_of(" \t");
      return rest.substr(0, last + 1);
    };
    if (line == "/quit") {
      screen_.Exit();
      return;
    }
    if (line == "/retry") {
      if (history_.empty()) {
        state_.notice = "nothing to retry";
        return;
      }
      send_prompt(history_.back());
      return;
    }
    if (line == "/export" || line.rfind("/export ", 0) == 0) {
      booth::export_command(state_, line, history_file_.parent_path(), cwd_);
      return;
    }
    if (line == "/close") {
      worker_.post([this, ctx = context()]() mutable { booth::close_session(ctx); }); return;
    }
    if (line == "/sessions") {
      picker_open_ = false;
      sessions_open_ = true;
      sessions_loading_ = true;
      sessions_index_ = 0;
      state_.history.clear();
      worker_.post([this, ctx = context()]() mutable { booth::list_sessions(ctx); });
      return;
    }
    if (line == "/model") {
      if (state_.models.empty()) {
        state_.notice = "no models yet; send a prompt first";
        return;
      }
      picker_open_ = true;
      sessions_open_ = false;
      picker_index_ = 0;
      for (std::size_t index = 0; index < state_.models.size(); ++index) {
        if (state_.models[index].name == state_.model) picker_index_ = index;
      }
      return;
    }
    if (const auto model = argument("/model ")) {
      if (model->empty()) {
        state_.notice = "usage: /model auto|<id>";
        return;
      }
      std::string value = *model;
      for (const auto& choice : state_.models) {
        if (choice.name == *model || choice.value == *model) {
          value = choice.value;
          break;
        }
      }
      state_.model = *model;
      std::string session;
      {
        std::lock_guard<std::mutex> lock(session_mutex_);
        session = session_;
      }
      if (!session.empty() && *model != "auto") {
        worker_.post([this, session, id = value, display = *model] {
          auto selected = supervisor_.request(
              "session/set_config_option", acp::set_config_params(session, "model", id),
              std::chrono::seconds(10));
          if (!selected) {
            publish(Event{Problem{selected.error().layer, selected.error().detail}});
            return;
          }
          publish(Event{SessionConfigChanged{SessionId{session}, "model", display}});
        });
      }
      state_.notice = "model " + *model;
      return;
    }
    state_.notice = "unknown command " + line;
  }
  PromptContext context() {
    return PromptContext{supervisor_,
                         state_,
                         session_mutex_,
                         session_,
                         cwd_,
                         sessions_dir_,
                         [this](Event event) { publish(std::move(event)); }};
  }
  void run_prompt(std::string text) { auto ctx = context(); booth::run_prompt(ctx, std::move(text)); }
  void apply_model(const booth::ModelChoice& choice) { worker_.post([this, ctx = context(), choice]() mutable { booth::apply_model(ctx, choice); }); }
  void cancel() {
    if (state_.permission) {
      const auto id = state_.permission->id.value;
      state_.permission.reset();
      supervisor_.answer_permission(id, std::nullopt);
    }
    std::string session;
    {
      std::lock_guard<std::mutex> lock(session_mutex_);
      session = session_;
    }
    if (!session.empty()) {
      supervisor_.notify("session/cancel", acp::cancel_params(session));
      state_.notice = "cancelled";
    }
  }
  void answer(const std::string& kind) {
    if (!state_.permission) return;
    const auto& request = *state_.permission;
    const auto pick = [&](const std::string& wanted) -> std::string {
      for (const auto& candidate : request.options) {
        if (candidate.kind == wanted) return candidate.id;
      }
      return {};
    };
    std::string option = pick(kind);
    if (option.empty()) {
      option = pick(kind.rfind("allow", 0) == 0 ? "allow_once" : "reject_once");
    }
    const std::optional<std::string> choice =
        option.empty() ? std::nullopt : std::optional<std::string>(option);
    const auto answered = supervisor_.answer_permission(request.id.value, choice);
    if (!answered) state_.notice = answered.error().detail;
    state_.permission.reset();
  }
  ScreenInteractive& screen_;
  Kernel& supervisor_;
  AppState& state_;
  Inbox& inbox_;
  Worker& worker_;
  booth::Editor editor_;
  std::string cwd_;
  std::filesystem::path history_file_;
  std::filesystem::path sessions_dir_;
  booth::GitStatus git_;
  std::string session_title_;
  bool picker_open_{false};
  std::size_t picker_index_{0};
  bool sessions_open_{false};
  bool sessions_loading_{false};
  std::size_t sessions_index_{0};
  std::atomic<bool> animate_{false};
  std::atomic<bool> alive_{true};
  std::thread ticker_;
  std::chrono::steady_clock::time_point busy_since_{};
  std::vector<std::string> history_;
  std::size_t history_pos_{0};
  KeyBindings keys_;
  booth::TranscriptScroll scroll_;
  booth::SlashMenu slash_;
  bool new_after_close_{false};
  std::deque<std::string> queued_;
  std::mutex session_mutex_;
  std::string session_;
};
}  // namespace

int tui_loop(ScreenInteractive& screen, Kernel& kernel, AppState& state, Inbox& inbox,
             const Config& config) {
  auto bindings = load_keybindings(config.state_dir / "keybindings.yml");
  if (!bindings) {
    std::cerr << "booth: " << to_string(bindings.error()) << " (using defaults)\n";
    bindings = KeyBindings{};
  }
  Worker worker;
  auto root = ftxui::Make<Root>(screen, kernel, state, inbox, worker,
                                std::filesystem::current_path().string(), config.history_file,
                                config.dsh_home / "sessions", *bindings);
  screen.Loop(root);
  root->shutdown();
  return 0;
}
}  // namespace booth
