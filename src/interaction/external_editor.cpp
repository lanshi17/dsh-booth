#include "interaction/external_editor.hpp"

#include <cerrno>
#include <cstdlib>
#include <fstream>
#include <string>
#include <vector>

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

// `$EDITOR` is an arbitrary command, so it gets the same quoting as the path.
std::string editor_command(const std::string& path) {
  const char* visual = std::getenv("VISUAL");
  const char* editor = std::getenv("EDITOR");
  const std::string binary = visual != nullptr && *visual != '\0'   ? visual
                             : editor != nullptr && *editor != '\0' ? editor
                                                                    : "vi";
  return binary + " " + shell_quote(path);
}

// Create the scratch file in the first directory that accepts it, then write
// the seed through the open descriptor: reopening the path would leave a
// window where another process could swap it for a symlink. TMPDIR comes
// first so a sandbox or remote host can choose a writable location.
std::optional<std::string> create_scratch(const std::string& initial) {
  std::vector<std::string> directories;
  const char* tmp = std::getenv("TMPDIR");
  if (tmp != nullptr && *tmp != '\0') directories.emplace_back(tmp);
  const char* runtime = std::getenv("XDG_RUNTIME_DIR");
  if (runtime != nullptr && *runtime != '\0') directories.emplace_back(runtime);
  directories.emplace_back("/tmp");
  directories.emplace_back("/var/tmp");

  for (const auto& directory : directories) {
    std::string pattern = directory + "/booth-edit-XXXXXX";
    std::vector<char> path(pattern.begin(), pattern.end());
    path.push_back('\0');
    const int fd = ::mkstemp(path.data());
    if (fd < 0) continue;
    std::size_t offset = 0;
    bool written = true;
    while (offset < initial.size()) {
      const ssize_t count = ::write(fd, initial.data() + offset, initial.size() - offset);
      if (count < 0 && errno == EINTR) continue;
      if (count <= 0) {
        written = false;
        break;
      }
      offset += static_cast<std::size_t>(count);
    }
    ::close(fd);
    if (!written) {
      ::unlink(path.data());
      continue;
    }
    return std::string(path.data());
  }
  return std::nullopt;
}

}  // namespace

std::optional<std::string> edit_externally(const std::string& initial) {
  auto path = create_scratch(initial);
  if (!path) return std::nullopt;

  const int status = std::system(editor_command(*path).c_str());

  std::ifstream input(*path, std::ios::binary);
  std::string content((std::istreambuf_iterator<char>(input)), std::istreambuf_iterator<char>());
  const bool read_ok = static_cast<bool>(input);
  input.close();
  ::unlink(path->c_str());
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
  // Re-entering the alternate screen after $EDITOR starts from a blank buffer;
  // request a full frame so the TUI repaints instead of leaving a gray screen.
  screen.RequestAnimationFrame();
  screen.PostEvent(ftxui::Event::Custom);
  return ok;
}

}  // namespace booth
