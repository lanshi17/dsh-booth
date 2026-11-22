#include "interaction/external_editor.hpp"

#include <cstdlib>
#include <fstream>
#include <string>
#include <unistd.h>

namespace booth {
namespace {

std::string shell_quote(const std::string& value) {
  std::string out = "'";
  for (const char ch : value) {
    if (ch == '\'') {
      out += "'\\''";
    } else {
      out += ch;
    }
  }
  out += "'";
  return out;
}

}  // namespace

std::optional<std::string> edit_externally(const std::string& initial) {
  char path[] = "/tmp/booth-edit-XXXXXX";
  const int fd = ::mkstemp(path);
  if (fd < 0) return std::nullopt;
  {
    std::ofstream output(path, std::ios::trunc);
    if (!output) {
      ::close(fd);
      ::unlink(path);
      return std::nullopt;
    }
    output << initial;
  }
  ::close(fd);

  const char* visual = std::getenv("VISUAL");
  const char* editor = std::getenv("EDITOR");
  const std::string command =
      std::string(visual != nullptr && *visual != '\0'   ? visual
                  : editor != nullptr && *editor != '\0' ? editor
                                                         : "vi") +
      " " + shell_quote(path);
  const int status = std::system(command.c_str());

  std::ifstream input(path, std::ios::binary);
  std::string content((std::istreambuf_iterator<char>(input)), std::istreambuf_iterator<char>());
  const bool read_ok = static_cast<bool>(input);
  input.close();
  ::unlink(path);
  if (status != 0 || !read_ok) return std::nullopt;
  if (!content.empty() && content.back() == '\n') content.pop_back();
  return content;
}

bool edit_in_screen(ftxui::ScreenInteractive& screen, std::string& content,
                    std::string& notice) {
  bool ok = false;
  screen.WithRestoredIO([&] {
    auto edited = edit_externally(content);
    if (!edited) {
      notice = "external editor failed";
      return;
    }
    content = *edited;
    ok = true;
  })();
  return ok;
}

}  // namespace booth
