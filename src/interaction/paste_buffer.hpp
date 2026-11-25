#pragma once

#include <string>

#include <ftxui/component/event.hpp>

namespace booth {

class Editor;

/// Bracketed-paste input mode: buffers pasted bytes and inserts them as one prompt edit.
class PasteBuffer {
 public:
  /// Returns true when the event belonged to a paste (or its start/end marker).
  bool feed(const ftxui::Event& event, Editor& editor);

 private:
  bool active_{false};
  std::string text_;
};

}  // namespace booth
