#include "interaction/open_link.hpp"

#include <cctype>
#include <string>

#if !defined(_WIN32)
#include <cerrno>
#include <sys/types.h>
#include <sys/wait.h>
#include <unistd.h>
#endif

namespace booth {
namespace {

std::string lower(std::string_view text) {
  std::string out(text);
  for (char& ch : out) {
    ch = static_cast<char>(std::tolower(static_cast<unsigned char>(ch)));
  }
  return out;
}

}  // namespace

bool is_openable_url(std::string_view url) {
  if (url.empty() || url.size() > 4096) return false;
  for (const char ch : url) {
    const auto byte = static_cast<unsigned char>(ch);
    if (byte <= 0x20 || byte == 0x7f) return false;
  }
  const auto colon = url.find(':');
  if (colon == std::string_view::npos || colon == 0) return false;
  const std::string scheme = lower(url.substr(0, colon));
  if (scheme == "mailto") return true;
  if (scheme != "http" && scheme != "https") return false;
  return url.substr(colon + 1, 2) == "//" && url.size() > colon + 3;
}

bool open_url(std::string_view url) {
  if (!is_openable_url(url)) return false;
#if defined(_WIN32)
  return false;
#else
  const std::string target(url);
  const pid_t pid = ::fork();
  if (pid < 0) return false;
  if (pid == 0) {
    // Double-fork so the browser's lifetime is independent of booth and the
    // root only reaps the immediate child.
    const pid_t grandchild = ::fork();
    if (grandchild != 0) ::_exit(grandchild < 0 ? 1 : 0);
    ::setsid();
#if defined(__APPLE__)
    ::execlp("open", "open", target.c_str(), static_cast<char*>(nullptr));
#else
    ::execlp("xdg-open", "xdg-open", target.c_str(), static_cast<char*>(nullptr));
#endif
    ::_exit(127);
  }
  int status = 0;
  while (::waitpid(pid, &status, 0) < 0) {
    if (errno != EINTR) return false;
  }
  return WIFEXITED(status) && WEXITSTATUS(status) == 0;
#endif
}

}  // namespace booth
