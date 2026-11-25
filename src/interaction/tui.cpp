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
#include <algorithm>
#include <cctype>
#include <atomic>
#include <cstdlib>
#include <chrono>
#include <condition_variable>
#include <cstddef>
#include <deque>
#include <set>
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
#include "interaction/help_scroll.hpp"
#include "interaction/keymap.hpp"
#include "interaction/editor.hpp"
#include "interaction/emoji.hpp"
#include "interaction/mention.hpp"
#include "interaction/mention_menu.hpp"
#include "interaction/mouse_actions.hpp"
#include "interaction/export.hpp"
#include "interaction/external_editor.hpp"
#include "interaction/clipboard.hpp"
#include "interaction/notify.hpp"
#include "interaction/history.hpp"
#include "interaction/history_search.hpp"
#include "interaction/word_completion.hpp"
#include "interaction/prompt.hpp"
#include "interaction/scroll.hpp"
#include "interaction/shell_command.hpp"
#include "interaction/link_picker.hpp"
#include "interaction/mouse_mode.hpp"
#include "interaction/commands_ui.hpp"
#include "interaction/error_view.hpp"
#include "interaction/copy_picker.hpp"
#include "interaction/draft.hpp"
#include "interaction/recall.hpp"
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
       std::filesystem::path sessions_dir, std::filesystem::path draft_file,
       std::filesystem::path socket_file, KeyBindings keys)
      : screen_(screen),
        supervisor_(supervisor),
        state_(state),
        inbox_(inbox),
        worker_(worker),
        editor_(state.composer),
        cwd_(std::move(cwd)),
        history_file_(std::move(history_file)),
        sessions_dir_(std::move(sessions_dir)),
        draft_file_(std::move(draft_file)),
        socket_file_(std::move(socket_file)),
        keys_(keys) {
    state_.composer = booth::load_draft(draft_file_); editor_.SetContent(state_.composer);
    mouse_enabled_ = booth::load_mouse_enabled(draft_file_.parent_path());
    if (const char* vim = ::getenv("BOOTH_VIM"); vim != nullptr && *vim != '\0' && *vim != '0') editor_.EnableVim(true);
    history_ = booth::load_history(history_file_); booth::load_file_commands(cwd_, draft_file_.parent_path());
    git_ = booth::read_git(cwd_);
    ticker_ = booth::start_ticker(screen_, animate_, alive_);
    worker_.post([this, ctx = context()]() mutable { booth::ensure_session(ctx); });
  }
  void shutdown() { booth::save_draft(draft_file_, state_.composer); cancel(); animate_ = false; alive_ = false; shell_job_.join(); if (ticker_.joinable()) ticker_.join(); worker_.stop(); }
  Element OnRender() override {
    mouse_.begin_frame();
    if (library_.visible()) return library_.view();
    if (state_.help) return booth::help_view(help_scroll_.offset(), keys_, &mouse_) | ftxui::size(ftxui::WIDTH, ftxui::EQUAL, 62) | ftxui::borderRounded | ftxui::center;
    return main_view();
  }
  bool OnEvent(ftxui::Event event) override {
    if (event == ftxui::Event::Custom) { drain(); return true; }
    if (paste_.feed(event, editor_)) return true;
    if (matches_chord(keys_.cancel, event)) {
      if (!state_.busy && !state_.permission && !state_.composer.empty()) {
        recall_.clear(state_.composer); state_.composer.clear(); editor_.Reset(); booth::save_draft(draft_file_, {});
        state_.notice = "draft cleared · Up recalls it";
      } else cancel();
      return true;
    }
    if (event.is_mouse() && !mouse_enabled_) return true;
    if (state_.help && event.is_mouse() && help_scroll_.wheel(event)) return true;
    if (event.is_mouse()) {
      const auto in = mouse_.handle(event, &screen_);
      if (in.kind == booth::MouseInput::Kind::Moved) { screen_.PostEvent(ftxui::Event::Custom); return true; }
      if (in.kind == booth::MouseInput::Kind::Click) {
        booth::MouseActions a{state_, editor_, slash_, files_, model_picker_, sessions_, history_search_, copy_picker_, mention_menu_,
          [this] { submit(); }, [this](const booth::ModelChoice& c) { apply_model(c); },
          [this](const booth::SessionRow& s) { state_.notice = "resuming…"; worker_.post([this, ctx = context(), s]() mutable { booth::resume_session(ctx, s); }); },
          [this](std::size_t line, std::size_t column) { auto rows = editor_.Lines(); if (line >= rows.size()) return; std::size_t at = std::min(column, rows[line].size()); for (std::size_t row = 0; row < line; ++row) at += rows[row].size() + 1; editor_.SetCursor(at); },
          [this](std::size_t entry) { booth::toggle_entry(state_, open_tools_, entry); },
          [this](const char* kind) { booth::answer_permission_choice(supervisor_, state_, kind); },
          [this] { state_.help = false; }};
        booth::click_mouse(in.hit, a);
        return true;
      }
    }
    if (state_.help) {
      if (help_scroll_.key(event)) return true;
      if (event == ftxui::Event::Escape || event == ftxui::Event::Return || event == ftxui::Event::Character('q')) state_.help = false;
      return true;
    }
    if (state_.permission) { booth::handle_permission_key(event, state_, supervisor_, keys_); return true; }
    if (library_.visible()) return booth::handle_library_event(event, state_, library_, model_picker_, editor_, {&show_thoughts_, &tools_hidden_, &tools_expanded_, &mouse_enabled_, &screen_, draft_file_.parent_path()});
    if (matches_chord(keys_.model_next, event)) { if (auto choice = booth::cycle_model(state_, 1)) apply_model(*choice); return true; }
    if (matches_chord(keys_.retry, event)) { if (!state_.busy && !history_.empty()) send_prompt(history_.back()); return true; }
    if (matches_chord(keys_.reasoning, event)) {
      const auto choice = booth::next_reasoning(state_.reasoning);
      if (!choice) {
        state_.notice = "no reasoning options";
        return true;
      }
      worker_.post([this, ctx = context(), id = state_.reasoning.id, choice = *choice]() mutable {
        booth::set_reasoning(ctx, id, choice);
      });
      state_.notice = "reasoning " + choice->name;
      return true;
    }
    if (sessions_.visible()) {
      switch (sessions_.key(event, state_.history)) {
        case booth::SessionPicker::Action::Refresh:
          worker_.post([this, ctx = context()]() mutable { booth::list_sessions(ctx); });
          break;
        case booth::SessionPicker::Action::Close:
          state_.notice = "closing…";
          worker_.post([this, ctx = context()]() mutable { booth::close_session(ctx); });
          break;
        case booth::SessionPicker::Action::Resume: {
          const auto selected = state_.history[sessions_.matches()[sessions_.index()]];
          state_.notice = "resuming…";
          worker_.post([this, ctx = context(), selected]() mutable {
            booth::resume_session(ctx, selected);
          });
          break;
        }
        case booth::SessionPicker::Action::None:
          break;
      }
      return true;
    }
    if (model_picker_.visible()) {
      if (const auto chosen = model_picker_.key(event, state_.models)) apply_model(*chosen);
      return true;
    }
    slash_.sync(state_.composer);
    if (slash_.visible(state_.composer, state_)) {
      const auto action = slash_.key(event, state_);
      if (action != booth::SlashAction::None) {
        editor_.SetContent(state_.composer);
        if (action == booth::SlashAction::Submit) submit();
        return true;
      }
    }
    files_.sync(state_.composer, cwd_);
    if (files_.visible() && files_.key(event, state_.composer)) { editor_.SetContent(state_.composer); return true; }
    switch (history_search_.handle(event, state_, history_, matches_chord(keys_.history, event))) {
      case booth::HistorySearch::Result::Accepted:
        editor_.SetContent(state_.composer);
        return true;
      case booth::HistorySearch::Result::Handled: return true;
      case booth::HistorySearch::Result::None: break;
    }
    if (copy_picker_.visible()) { copy_picker_.key(event, state_); return true; }
    if (links_.visible()) { links_.key(event, state_); return true; }
    mention_menu_.update(editor_.Lines(), editor_.CursorLine(), editor_.CursorColumn(), state_.models, efforts());
    if (!editor_.VimNormal() && mention_menu_.visible()) { if (auto result = mention_menu_.key(event, state_.composer)) { state_.composer = result->content; editor_.SetContent(state_.composer); editor_.SetCursor(result->cursor); return true; } }
    if (event == ftxui::Event::Tab && !slash_.visible(state_.composer, state_) && !files_.visible() && !mention_menu_.visible() && !editor_.VimNormal()) {
      std::vector<std::string> transcript; for (const auto& entry : state_.entries) if (entry.kind == EntryKind::User || entry.kind == EntryKind::Assistant) transcript.push_back(entry.text);
      if (const auto suffix = booth::word_completion_suffix(state_.composer, editor_.CursorOffset(), booth::build_word_vocabulary(history_, transcript))) return editor_.HandleEvent(ftxui::Event::Character(*suffix));
    }
    if (matches_chord(keys_.help, event)) { state_.help = true; help_scroll_.reset(); return true; }
    if (matches_chord(keys_.model, event)) { if (!state_.models.empty()) model_picker_.open(state_.models, state_.model); return true; }
    if (matches_chord(keys_.sessions, event)) { handle_command("/sessions"); return true; }
    if (matches_chord(keys_.copy, event)) { handle_command("/copy"); return true; }
    if (matches_chord(keys_.links, event)) { links_.open(state_); return true; }
    if (!matches_chord(keys_.editor, event) &&
        booth::view_toggle(event, keys_, tools_expanded_, tools_hidden_, show_thoughts_, state_)) {
      return true;
    }
    if (event.is_mouse()) {
      const auto& mouse = event.mouse();
      if (mouse.button == ftxui::Mouse::WheelUp) { scroll_.up(state_.entries.size(), 3); return true; } if (mouse.button == ftxui::Mouse::WheelDown) { scroll_.down(3); return true; }
    }
    if (matches_chord(keys_.page_up, event)) { scroll_.page_up(state_.entries.size()); return true; }
    if (matches_chord(keys_.page_down, event)) { scroll_.page_down(); return true; }
    if (matches_chord(keys_.paste_image, event)) { if (auto image = booth::clipboard_image()) { images_.push_back(*image); state_.notice = "image attached"; } else state_.notice = "no image in clipboard"; return true; }
    if (matches_chord(keys_.editor, event)) {
      if (booth::edit_in_screen(screen_, state_.composer, state_.notice)) {
        editor_.SetContent(state_.composer);
      }
      return true;
    }
    if (matches_chord(keys_.quit, event)) {
      screen_.Exit();
      return true;
    }
    if (matches_chord(keys_.dequeue, event) && booth::dequeue_prompt(state_, queued_)) {
      editor_.SetContent(state_.composer);
      return true;
    }
    if (event == ftxui::Event::ArrowUp && state_.composer.empty() && !editor_.VimNormal()) {
      if (const auto text = recall_.up(history_)) { state_.composer = *text; editor_.SetContent(*text); }
      return true;
    }
    if (event == ftxui::Event::ArrowDown) {
      if (const auto text = recall_.down(history_)) { state_.composer = *text; editor_.SetContent(*text); }
      return true;
    }
    if (event == ftxui::Event::Return) { submit(); return true; }
    if (editor_.HandleEvent(event)) return true;
    return false;
  }
 private:
  Element main_view() {
    Elements lines;
    const auto window = scroll_.window(state_.entries.size());
    for (std::size_t index = window.begin; index < window.end; ++index) {
      const auto& entry = state_.entries[index];
      if (entry.kind == EntryKind::Thought && !show_thoughts_) continue;
      if (entry.kind == EntryKind::Tool && tools_hidden_) continue;
      lines.push_back(mouse_.zone(booth::entry_element(entry, tools_expanded_ || open_tools_.count(index) != 0), MouseSurface::Transcript, index));
    }
    if (window.scroll == 0 && !state_.streaming.empty()) {
      lines.push_back(booth::markdown_view(state_.streaming, false));
    }
    if (state_.busy && window.scroll == 0) {
      const auto elapsed = std::chrono::steady_clock::now() - busy_since_;
      const auto seconds = std::chrono::duration_cast<std::chrono::seconds>(elapsed).count();
      const auto ms = std::chrono::duration_cast<std::chrono::milliseconds>(elapsed).count();
      const std::string activity = booth::active_activity(state_);
      lines.push_back(ftxui::text(std::string("  ") + kSpinner[(ms / 120) % 8] + " " +
                                  activity + " · " + std::to_string(seconds) + "s") |
                      ftxui::color(ftxui::Color::RGB(228, 228, 228)));
    }
    Elements view;
    view.push_back(ftxui::vbox(std::move(lines)) | ftxui::flex);
    if (state_.permission) view.push_back(booth::permission_view(*state_.permission, &mouse_, state_.permission_expanded) | ftxui::size(ftxui::WIDTH, ftxui::EQUAL, 62) | ftxui::borderRounded | ftxui::center);
    if (!state_.notice.empty()) view.push_back(ftxui::paragraph(state_.notice.text) | ftxui::color(state_.notice.error ? ftxui::Color::RGB(255, 95, 95) : ftxui::Color::RGB(150, 150, 150)));
    if (window.scroll > 0) {
      view.push_back(ftxui::text("  ↑ " + std::to_string(window.scroll) +
                                 " entries back · PageDown to follow") |
                     ftxui::color(ftxui::Color::RGB(150, 150, 150)));
    }
    if (auto hud = booth::todo_hud(state_.todos)) view.push_back(std::move(*hud));
    if (!state_.error.empty()) view.push_back(booth::error_banner_view(state_.error));
    if (!queued_.empty()) view.push_back(booth::queued_view(queued_));
    if (copy_picker_.visible()) view.push_back(booth::copy_picker_view(copy_picker_.items(), copy_picker_.matches(),
                                                copy_picker_.index(), copy_picker_.query(), &mouse_));
    if (links_.visible()) view.push_back(booth::link_picker_view(links_.items(), links_.index()));
    if (history_search_.visible()) view.push_back(booth::history_search_view(history_search_.query(), history_search_.matches(), history_search_.index(), &mouse_));
    if (sessions_.visible()) {
      view.push_back(
          booth::session_picker_view(state_.history, sessions_.matches(), sessions_.index(),
                                     sessions_.loading(), sessions_.query(), &mouse_));
    }
    if (model_picker_.visible()) view.push_back(booth::model_picker_view(state_.models, model_picker_.matches(), model_picker_.index(), state_.model, model_picker_.query(), &mouse_));
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
    mention_menu_.update(editor_.Lines(), editor_.CursorLine(), editor_.CursorColumn(), state_.models, efforts());
    view.push_back(booth::composer_view(state_, cwd_, git_, title, busy_seconds, spinner,
                                        editor_.Lines(), editor_.CursorLine(),
                                        editor_.CursorColumn(), state_.reasoning.current_name,
                                        queued_.size(), editor_.ModeLabel(), images_.size(), &mouse_, editor_.VimSelection(), editor_.VimMode() != booth::vim::Mode::Insert));
    slash_.sync(state_.composer);
    if (!editor_.VimNormal() && mention_menu_.visible()) view.push_back(mention_menu_.view(&mouse_));
    if (slash_.visible(state_.composer, state_)) {
      view.push_back(booth::command_menu_view(state_, state_.composer, slash_.index(), &mouse_));
    }
    files_.sync(state_.composer, cwd_);
    if (files_.visible()) view.push_back(booth::file_menu_view(files_.query(), files_.matches(), files_.index(), &mouse_));
    return ftxui::vbox(std::move(view));
  }
  void drain() {
    std::deque<Event> events;
    {
      std::lock_guard<std::mutex> lock(inbox_.mutex);
      events.swap(inbox_.events);
    }
    const std::size_t before = state_.entries.size();
    for (auto& result : shell_job_.take_completed()) state_.entries.push_back(local_python_ ? booth::python_tool_entry(result, "local-shell-" + std::to_string(++shell_id_)) : booth::shell_tool_entry(result, "local-shell-" + std::to_string(++shell_id_)));
    bool switched = false;
    for (auto& event : events) {
      if (std::holds_alternative<SessionSwitched>(event.value)) switched = true;
      const bool turn_done = std::holds_alternative<StreamEnd>(event.value);
      const bool listed = std::holds_alternative<SessionsListed>(event.value);
      if (std::holds_alternative<Problem>(event.value) && state_.busy) turn_failed_ = true;
      if (const auto* permission = std::get_if<PermissionRequest>(&event.value);
          permission && booth::notifications_enabled(draft_file_.parent_path()))
        booth::send_notification("booth", "permission needed: " + permission->tool);
      if (turn_done) {
        const auto elapsed = std::chrono::steady_clock::now() - busy_since_;
        if (state_.busy && elapsed >= std::chrono::seconds(5) &&
            booth::notifications_enabled(draft_file_.parent_path()))
          booth::send_notification("booth", std::string("turn ") + (turn_failed_ ? "failed" : "complete") + " · " + std::to_string(std::chrono::duration_cast<std::chrono::seconds>(elapsed).count()) + "s");
        turn_failed_ = false;
      }
      const bool closed = std::holds_alternative<SessionClosed>(event.value);
      apply(state_, event);
      if (listed) sessions_.loaded(state_.history);
      if (std::holds_alternative<Problem>(event.value)) new_after_close_ = false;
      if (closed) {
        session_title_.clear();
        queued_.clear();
        mention_registry_.clear();
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
      const auto path = mention_path(state_.session.empty() ? std::string{"none"} : state_.session);
      if (std::filesystem::exists(path)) { mention_registry_.clear(); mention_registry_.load(path); }
      else if (!mention_registry_.mentions().empty()) mention_registry_.save(path);
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
    if (text.front() == '!' || text.front() == '$') { submit_local(text, text.front() == '$'); return; }
    if (state_.busy) {
      if (command && booth::local_command(text)) { state_.composer.clear(); editor_.Reset(); handle_command(text); }
      else if (!command) { state_.composer.clear(); editor_.Reset(); queued_.push_back(std::move(text)); state_.notice = std::to_string(queued_.size()) + " queued"; }
      else state_.notice = "busy: local commands and /queue still work";
      return;
    }
    state_.composer.clear(); editor_.Reset(); booth::save_draft(draft_file_, {});
    if (command) {
      handle_command(text);
      return;
    }
    send_prompt(std::move(text));
  }
  void submit_local(const std::string& submitted, bool python) {
    std::string command = submitted.substr(1);
    if (command.find_first_not_of(" \t\r\n") == std::string::npos) { state_.notice = python ? "usage: $<python>" : "usage: !<command>"; return; }
    if (shell_job_.busy()) { state_.notice = "a local command is already running"; return; }
    state_.composer.clear(); editor_.Reset(); booth::save_draft(draft_file_, {});
    TranscriptEntry user; user.kind = EntryKind::User; user.text = submitted; state_.entries.push_back(std::move(user));
    scroll_.follow(); state_.notice.clear(); local_python_ = python;
    const std::string wire = python ? booth::python_shell_command(command) : command;
    if (!shell_job_.start(std::move(wire), cwd_, [this] { if (alive_) screen_.PostEvent(ftxui::Event::Custom); }, std::move(command)))
      state_.notice = "could not start local command";
  }
  void send_prompt(std::string text) {
    turn_failed_ = false;
    state_.notice.clear(); state_.error.clear(); state_.busy = true; animate_ = true;
    text = booth::expand_emojis(text);
    const std::string display = text;
    std::vector<std::string> efforts; for (const auto& choice : state_.reasoning.choices) efforts.push_back(choice.value);
    std::string wire = mention_registry_.expand(text, state_.models, efforts);
    if (!mention_registry_.mentions().empty() && !state_.session.empty()) mention_registry_.save(mention_path(state_.session));
    if (session_title_.empty()) session_title_ = display;
    busy_since_ = std::chrono::steady_clock::now();
    TranscriptEntry echo; echo.kind = EntryKind::User; echo.text = display;
    publish(Event{std::move(echo)});
    scroll_.follow();
    history_.push_back(display); recall_.reset();
    booth::append_history(history_file_, display);
    std::vector<std::pair<std::string, std::string>> resources;
    for (const auto& attachment : state_.attachments) resources.emplace_back(attachment.name, attachment.uri);
    for (auto& link : booth::file_links(display, cwd_)) resources.push_back(std::move(link));
    state_.attachments.clear();
    worker_.post([this, text = std::move(wire), resources = std::move(resources), images = std::move(images_)]() mutable {
      run_prompt(std::move(text), resources, images);
    });
  }
  void handle_command(std::string line) {
    line = booth::normalize_command(std::move(line));
    if (auto prompt = booth::file_command_prompt(line)) { send_prompt(std::move(*prompt)); return; }
    if (booth::handle_library_command(line, state_, library_, cwd_,
                                      {&show_thoughts_, &tools_hidden_, &tools_expanded_, &mouse_enabled_, &screen_, draft_file_.parent_path()}, editor_)) return;
    if (booth::handle_local_command(state_, scroll_, transcript_search_, queued_.size(), cwd_,
                                    draft_file_.parent_path(), line, show_thoughts_, tools_hidden_)) {
      if (line == "/help") help_scroll_.reset();
      if (line == "/theme" || line.rfind("/theme ", 0) == 0) screen_.PostEvent(ftxui::Event::Custom);
      return;
    }
    if (line == "/acp") { state_.notice = booth::acp_text(state_, supervisor_.connected(), socket_file_.string()); return; }
    if (line == "/cancel") { cancel(); state_.notice = "cancelled"; return; }
    if (line == "/copy") { if (!copy_picker_.open(state_)) state_.notice = "nothing to copy"; return; }
    if (line == "/links") { links_.open(state_); return; }
    if (line == "/mouse" || line.rfind("/mouse ", 0) == 0) { mouse_enabled_ = booth::handle_mouse_command(line, screen_, state_, draft_file_.parent_path()); return; }
    if (line == "/queue" || line.rfind("/queue ", 0) == 0) { booth::handle_queue_command(state_, queued_, line); return; }
    if (line == "/effort" || line.rfind("/effort ", 0) == 0) {
      booth::handle_effort_command(state_, line, [this](const std::string& id, const ModelChoice& picked) {
        worker_.post([this, ctx = context(), id, choice = picked]() mutable { booth::set_reasoning(ctx, id, choice); });
      });
      return;
    }
    if (line == "/new") { if (booth::start_new_session(state_, scroll_, new_after_close_)) worker_.post([this, ctx = context()]() mutable { booth::close_session(ctx); }); return; }
    const auto argument = [&](std::string_view prefix) -> std::optional<std::string> {
      if (line.rfind(prefix, 0) != 0) return std::nullopt;
      std::string rest = line.substr(prefix.size());
      const auto first = rest.find_first_not_of(" \t");
      if (first == std::string::npos) return std::string{};
      rest = rest.substr(first);
      const auto last = rest.find_last_not_of(" \t");
      return rest.substr(0, last + 1);
    };
    if (line == "/quit") { screen_.Exit(); return; }
    if (line == "/retry") { if (history_.empty()) { state_.notice = "nothing to retry"; return; } send_prompt(history_.back()); return; }
    if (line == "/export" || line.rfind("/export ", 0) == 0) {
      booth::export_command(state_, line, history_file_.parent_path(), cwd_);
      return;
    }
    if (line == "/close") {
      worker_.post([this, ctx = context()]() mutable { booth::close_session(ctx); }); return;
    }
    if (line == "/sessions") { model_picker_.close(); sessions_.open(draft_file_.parent_path() / "session-pins"); state_.history.clear(); worker_.post([this, ctx = context()]() mutable { booth::list_sessions(ctx); }); return; }
    if (line == "/model") { if (state_.models.empty()) { state_.notice = "no models yet; send a prompt first"; return; } sessions_.close(); model_picker_.open(state_.models, state_.model); return; }
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
  std::filesystem::path mention_path(const std::string& session) const { return draft_file_.parent_path() / ("mentions-" + session + ".json"); }
  std::vector<std::string> efforts() const { std::vector<std::string> out; for (const auto& choice : state_.reasoning.choices) out.push_back(choice.value); return out; }
  PromptContext context() {
    return PromptContext{supervisor_,
                         state_,
                         session_mutex_,
                         session_,
                         cwd_,
                         sessions_dir_,
                         [this](Event event) { publish(std::move(event)); }};
  }
  void run_prompt(std::string text,
                  const std::vector<std::pair<std::string, std::string>>& resources = {},
                  const std::vector<booth::acp::ImageAttachment>& images = {}) {
    auto ctx = context(); booth::run_prompt(ctx, std::move(text), resources, images);
  }
  void apply_model(const booth::ModelChoice& choice) { worker_.post([this, ctx = context(), choice]() mutable { booth::apply_model(ctx, choice); }); }
  void cancel() { auto ctx = context(); booth::cancel_turn(ctx); }
  ScreenInteractive& screen_;
  Kernel& supervisor_;
  AppState& state_;
  Inbox& inbox_;
  Worker& worker_;
  booth::ShellCommandJob shell_job_;
  std::size_t shell_id_{0};
  booth::Editor editor_;
  std::string cwd_;
  std::filesystem::path history_file_;
  std::filesystem::path sessions_dir_;
  std::filesystem::path draft_file_;
  std::filesystem::path socket_file_;
  booth::GitStatus git_;
  booth::HelpScroll help_scroll_;
  std::string session_title_;
  std::atomic<bool> animate_{false};
  std::atomic<bool> alive_{true};
  std::thread ticker_;
  std::chrono::steady_clock::time_point busy_since_{};
  bool turn_failed_{false};
  bool mouse_enabled_{true};
  bool local_python_{false};
  std::vector<std::string> history_;
  booth::PromptRecall recall_;
  KeyBindings keys_;
  booth::TranscriptScroll scroll_;
  booth::SlashMenu slash_;
  booth::SessionPicker sessions_;
  booth::ModelPicker model_picker_;
  booth::HistorySearch history_search_;
  booth::CopyPicker copy_picker_;
  booth::LinkPicker links_;
  booth::FileMenu files_;
  booth::LibraryOverlay library_;
  booth::TranscriptSearch transcript_search_;
  bool tools_expanded_{false}, tools_hidden_{false}; booth::PasteBuffer paste_;
  bool show_thoughts_{false};
  bool new_after_close_{false};
  std::deque<std::string> queued_;
  booth::MentionRegistry mention_registry_;
  booth::MouseRouter mouse_;
  booth::MentionMenu mention_menu_;
  std::vector<booth::acp::ImageAttachment> images_;
  std::set<std::size_t> open_tools_;
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
  const bool mouse = booth::load_mouse_enabled(config.state_dir); screen.TrackMouse(mouse);
  if (!mouse) std::cout << booth::mouse_disable_sequence() << std::flush;
  auto root = ftxui::Make<Root>(screen, kernel, state, inbox, worker,
                                std::filesystem::current_path().string(), config.history_file,
                                config.dsh_home / "sessions", config.state_dir / "draft",
                                config.socket_file, *bindings);
  std::cout << "\x1b[?2004h" << std::flush; screen.Loop(root); std::cout << "\x1b[?2004l" << std::flush;
  root->shutdown();
  return 0;
}
}  // namespace booth
