// Standalone uhid owner for ROMs where Shizuku user services cannot start
// (MediaTek's LoadedApk.makeApplicationInner NPE, Shizuku issues #1198/#2048).
//
// The app starts this binary through Shizuku's remote process API, so it runs
// as uid shell with the uhid group, then feeds it length-prefixed frames on
// stdin:
//
//   struct frame { uint32_t magic; uint32_t type; uint32_t size; uint8_t data[]; }
//   magic 0x50434544
//   type 1 CREATE  (HID report descriptor for the virtual stylus)
//   type 2 INPUT   (one raw stylus report, report ID included)
//   type 3 DESTROY (stylus)
//   type 4 ACTION  (u32: 1 = backspace, 2 = Ctrl+Z)
//
// Replies are newline-terminated lines on stdout: READY, CREATED <fd|err>,
// DESTROYED, KEY, KBD_CREATED, KBD_DESTROYED, EVENT <type>.
//
// Actions use a virtual keyboard that is created on demand and destroyed after
// 1.5 s idle, so Android never treats a hardware keyboard as permanently
// present (which would hide the on-screen keyboard).

#include <errno.h>
#include <fcntl.h>
#include <poll.h>
#include <signal.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#include <unistd.h>

#include <linux/input.h>
#include <linux/uhid.h>

#define FRAME_MAGIC 0x50434544u
#define FRAME_CREATE 1
#define FRAME_INPUT 2
#define FRAME_DESTROY 3
#define FRAME_ACTION 4
#define HEADER_SIZE 12
#define MAX_PAYLOAD 4096
#define MAX_BUFFER (HEADER_SIZE + MAX_PAYLOAD)

#define ACTION_BACKSPACE 1
#define ACTION_UNDO 2
#define ACTION_QUEUE 8
#define KEYBOARD_IDLE_MS 1500

static int pen_fd = -1;
static int kbd_fd = -1;
static int kbd_open;
static long kbd_last_use;
static int pending[ACTION_QUEUE];
static int pending_count;

static const unsigned char keyboard_descriptor[] = {
        0x05, 0x01, 0x09, 0x06, 0xa1, 0x01,       // Keyboard application
        0x05, 0x07, 0x19, 0xe0, 0x29, 0xe7,       // Modifiers 0xE0..0xE7
        0x15, 0x00, 0x25, 0x01, 0x75, 0x01, 0x95, 0x08, 0x81, 0x02,
        0x95, 0x01, 0x75, 0x08, 0x81, 0x01,       // Reserved byte
        0x05, 0x07, 0x19, 0x00, 0x29, 0x65,       // Keys 0..0x65
        0x15, 0x00, 0x25, 0x65, 0x75, 0x08, 0x95, 0x06, 0x81, 0x00,
        0xc0
};

static uint32_t read_u32(const unsigned char *p) {
    uint32_t value;
    memcpy(&value, p, sizeof(value));
    return value;
}

static long now_ms(void) {
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return ts.tv_sec * 1000L + ts.tv_nsec / 1000000L;
}

static void reply(const char *line) {
    write(STDOUT_FILENO, line, strlen(line));
}

static int write_all(int fd, const void *data, size_t length) {
    const unsigned char *p = data;
    while (length > 0) {
        ssize_t n = write(fd, p, length);
        if (n < 0) {
            if (errno == EINTR) continue;
            return -errno;
        }
        p += n;
        length -= (size_t) n;
    }
    return 0;
}

static int write_event(int fd, const struct uhid_event *ev) {
    return write_all(fd, ev, sizeof(*ev));
}

static void close_pen(void) {
    if (pen_fd >= 0) {
        struct uhid_event ev;
        memset(&ev, 0, sizeof(ev));
        ev.type = UHID_DESTROY;
        write_event(pen_fd, &ev);
        close(pen_fd);
        pen_fd = -1;
        reply("DESTROYED\n");
    }
}

static void close_keyboard(void) {
    if (kbd_fd >= 0) {
        struct uhid_event ev;
        memset(&ev, 0, sizeof(ev));
        ev.type = UHID_DESTROY;
        write_event(kbd_fd, &ev);
        close(kbd_fd);
        kbd_fd = -1;
        kbd_open = 0;
        pending_count = 0;
        reply("KBD_DESTROYED\n");
    }
}

static int create_uhid(const char *name, const unsigned char *descriptor, size_t length) {
    char line[64];
    int fd = open("/dev/uhid", O_RDWR | O_CLOEXEC);
    if (fd < 0) {
        snprintf(line, sizeof(line), "CREATED -%d\n", errno);
        reply(line);
        return -1;
    }

    struct uhid_event ev;
    memset(&ev, 0, sizeof(ev));
    ev.type = UHID_CREATE2;
    strncpy((char *) ev.u.create2.name, name, sizeof(ev.u.create2.name) - 1);
    strncpy((char *) ev.u.create2.phys, "decopad", sizeof(ev.u.create2.phys) - 1);
    strncpy((char *) ev.u.create2.uniq, "decopad-uhid", sizeof(ev.u.create2.uniq) - 1);
    memcpy(ev.u.create2.rd_data, descriptor, length);
    ev.u.create2.rd_size = (__u16) length;
    ev.u.create2.bus = BUS_USB;
    ev.u.create2.vendor = 0x28bd;
    ev.u.create2.product = 0x2904;
    ev.u.create2.version = 0x0100;

    int err = write_event(fd, &ev);
    if (err != 0) {
        close(fd);
        return err;
    }
    return fd;
}

static void create_pen(const unsigned char *descriptor, size_t length) {
    char line[64];
    close_pen();
    if (length == 0 || length > sizeof(((struct uhid_create2_req *) 0)->rd_data)) {
        reply("CREATED -22\n");
        return;
    }
    pen_fd = create_uhid("XPPen Deco 640 Pen (decopad)", descriptor, length);
    snprintf(line, sizeof(line), "CREATED %d\n", pen_fd);
    reply(line);
}

static void input_report(const unsigned char *report, size_t length) {
    if (pen_fd < 0 || length == 0 || length > UHID_DATA_MAX) return;
    struct uhid_event ev;
    memset(&ev, 0, sizeof(ev));
    ev.type = UHID_INPUT2;
    ev.u.input2.size = (__u16) length;
    memcpy(ev.u.input2.data, report, length);
    write_event(pen_fd, &ev);
}

static void keyboard_report(const unsigned char *report) {
    if (kbd_fd < 0) return;
    struct uhid_event ev;
    memset(&ev, 0, sizeof(ev));
    ev.type = UHID_INPUT2;
    ev.u.input2.size = 8;
    memcpy(ev.u.input2.data, report, 8);
    write_event(kbd_fd, &ev);
}

static void tap_action(int action) {
    unsigned char down[8] = {0};
    if (action == ACTION_BACKSPACE) {
        down[2] = 0x2a;                     // Keyboard Backspace
    } else if (action == ACTION_UNDO) {
        down[0] = 0x01;                     // Left Ctrl
        down[2] = 0x1d;                     // Z
    } else {
        return;
    }
    keyboard_report(down);
    usleep(20000);
    unsigned char up[8] = {0};
    keyboard_report(up);
    kbd_last_use = now_ms();
}

static void flush_pending(void) {
    for (int i = 0; i < pending_count; i++) tap_action(pending[i]);
    pending_count = 0;
}

static void request_action(int action) {
    if (action != ACTION_BACKSPACE && action != ACTION_UNDO) return;

    if (kbd_fd < 0) {
        kbd_fd = create_uhid("decopad keyboard", keyboard_descriptor,
                             sizeof(keyboard_descriptor));
        kbd_open = 0;
        kbd_last_use = now_ms();
        reply(kbd_fd >= 0 ? "KBD_CREATED\n" : "KBD_FAILED\n");
        if (kbd_fd < 0) return;
    }

    if (kbd_open) {
        tap_action(action);
        reply("KEY\n");
    } else if (pending_count < ACTION_QUEUE) {
        pending[pending_count++] = action;
        reply("KEY queued\n");
    }
}

static void drain_fd(int fd, int is_keyboard) {
    struct uhid_event ev;
    ssize_t n = read(fd, &ev, sizeof(ev));
    if (n != (ssize_t) sizeof(ev)) return;

    if (is_keyboard && ev.type == UHID_OPEN) {
        kbd_open = 1;
        flush_pending();
        return;
    }

    if (ev.type == UHID_GET_REPORT) {
        struct uhid_event answer;
        memset(&answer, 0, sizeof(answer));
        answer.type = UHID_GET_REPORT_REPLY;
        answer.u.get_report_reply.id = ev.u.get_report.id;
        answer.u.get_report_reply.err = 1;
        write_event(fd, &answer);
    }
    char line[32];
    snprintf(line, sizeof(line), "EVENT %u\n", (unsigned) ev.type);
    reply(line);
}

int main(void) {
    signal(SIGPIPE, SIG_IGN);
    signal(SIGCHLD, SIG_IGN);
    reply("READY\n");

    unsigned char buffer[MAX_BUFFER * 2];
    size_t used = 0;

    for (;;) {
        struct pollfd fds[3];
        int count = 0;
        fds[count].fd = STDIN_FILENO;
        fds[count].events = POLLIN;
        fds[count].revents = 0;
        count++;
        if (pen_fd >= 0) {
            fds[count].fd = pen_fd;
            fds[count].events = POLLIN;
            fds[count].revents = 0;
            count++;
        }
        if (kbd_fd >= 0) {
            fds[count].fd = kbd_fd;
            fds[count].events = POLLIN;
            fds[count].revents = 0;
            count++;
        }

        int ready = poll(fds, count, 200);
        if (ready < 0) {
            if (errno == EINTR) continue;
            break;
        }

        for (int i = 1; i < count; i++) {
            if (fds[i].revents & POLLIN) drain_fd(fds[i].fd, fds[i].fd == kbd_fd);
        }

        if (kbd_fd >= 0 && pending_count == 0
                && now_ms() - kbd_last_use > KEYBOARD_IDLE_MS) {
            close_keyboard();
        }

        if (fds[0].revents & (POLLIN | POLLHUP)) {
            if (used >= sizeof(buffer)) used = 0;
            ssize_t n = read(STDIN_FILENO, buffer + used, sizeof(buffer) - used);
            if (n <= 0) break;
            used += (size_t) n;

            size_t offset = 0;
            while (used - offset >= HEADER_SIZE) {
                uint32_t magic = read_u32(buffer + offset);
                uint32_t type = read_u32(buffer + offset + 4);
                uint32_t size = read_u32(buffer + offset + 8);
                if (magic != FRAME_MAGIC || size > MAX_PAYLOAD) {
                    used = 0;
                    break;
                }
                if (used - offset < HEADER_SIZE + size) break;

                const unsigned char *payload = buffer + offset + HEADER_SIZE;
                if (type == FRAME_CREATE) {
                    create_pen(payload, size);
                } else if (type == FRAME_INPUT) {
                    input_report(payload, size);
                } else if (type == FRAME_DESTROY) {
                    close_pen();
                } else if (type == FRAME_ACTION && size >= 4) {
                    request_action((int) read_u32(payload));
                }
                offset += HEADER_SIZE + size;
            }

            if (offset > 0 && offset < used) memmove(buffer, buffer + offset, used - offset);
            if (offset > 0) used -= offset;
        }
    }

    close_pen();
    close_keyboard();
    return 0;
}
