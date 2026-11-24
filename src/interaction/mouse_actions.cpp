#include "interaction/mouse_actions.hpp"

#include "interaction/open_link.hpp"

namespace booth {

bool click_mouse(const MouseHit& hit, MouseActions& a) {
  switch (hit.surface) {
    case MouseSurface::Slash: {
      if (!a.slash.visible(a.state.composer)) return false;
      const auto action = a.slash.click(hit.index, a.state);
      if (action == SlashAction::None) return false;
      a.editor.SetContent(a.state.composer);
      if (action == SlashAction::Submit && a.submit) a.submit();
      return true;
    }
    case MouseSurface::File:
      if (!a.files.click(hit.index, a.state.composer)) return false;
      a.editor.SetContent(a.state.composer);
      return true;
    case MouseSurface::Model:
      if (const auto chosen = a.model.click(hit.index, a.state.models); chosen && a.apply_model) {
        a.apply_model(*chosen);
        return true;
      }
      return false;
    case MouseSurface::Session:
      if (a.sessions.click(hit.index, a.state.history) != SessionPicker::Action::Resume) {
        return false;
      }
      if (a.resume && a.sessions.index() < a.sessions.matches().size()) {
        a.resume(a.state.history[a.sessions.matches()[a.sessions.index()]]);
      }
      return true;
    case MouseSurface::History:
      if (!a.history.click(hit.index, a.state)) return false;
      a.editor.SetContent(a.state.composer);
      return true;
    case MouseSurface::Copy:
      return a.copy.click(hit.index, a.state);
    case MouseSurface::Mention: {
      const auto result = a.mention.click(hit.index, a.state.composer);
      if (!result) return false;
      a.state.composer = result->content;
      a.editor.SetContent(a.state.composer);
      a.editor.SetCursor(result->cursor);
      return true;
    }
    case MouseSurface::Composer:
      if (a.cursor_to) a.cursor_to(hit.index, hit.column);
      return true;
    case MouseSurface::Permission: {
      if (!a.permission || !a.state.permission ||
          hit.index >= a.state.permission->options.size()) {
        return false;
      }
      a.permission(a.state.permission->options[hit.index].kind.c_str());
      return true;
    }
    case MouseSurface::Help:
      if (a.close_help) a.close_help();
      return true;
    case MouseSurface::Transcript:
      if (a.toggle_tool) a.toggle_tool(hit.index);
      return true;
    case MouseSurface::Link:
      if (open_url(hit.url)) a.state.notice = "opened " + hit.url;
      return true;
    case MouseSurface::None:
      return false;
  }
  return false;
}

}  // namespace booth
