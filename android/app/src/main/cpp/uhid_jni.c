// Owns /dev/uhid inside the Shizuku user service (uid shell, group uhid).
//
// uhid is an ioctl-less char device: every message is exactly one
// struct uhid_event written to /dev/uhid, and kernel events are read back.
// The uhid driver rejects writes whose size is not sizeof(struct uhid_event).

#include <jni.h>
#include <errno.h>
#include <fcntl.h>
#include <poll.h>
#include <pthread.h>
#include <string.h>
#include <unistd.h>

#include <linux/input.h>
#include <linux/uhid.h>
#include <android/log.h>

#define TAG "decopad-uhid"
#define LOGI(...) __android_log_print(ANDROID_LOG_INFO, TAG, __VA_ARGS__)
#define LOGW(...) __android_log_print(ANDROID_LOG_WARN, TAG, __VA_ARGS__)

static pthread_mutex_t g_lock = PTHREAD_MUTEX_INITIALIZER;
static int g_fd = -1;
static pthread_t g_reader;
static int g_reader_running;
static int g_reader_started;

static int write_event_locked(const struct uhid_event *ev) {
    const unsigned char *p = (const unsigned char *) ev;
    size_t left = sizeof(*ev);

    while (left > 0) {
        ssize_t n = write(g_fd, p, left);
        if (n < 0) {
            if (errno == EINTR) continue;
            return -errno;
        }
        p += n;
        left -= (size_t) n;
    }
    return 0;
}

static void *reader_main(void *unused) {
    (void) unused;
    struct uhid_event ev;
    struct uhid_event reply;

    for (;;) {
        pthread_mutex_lock(&g_lock);
        int running = g_reader_running;
        int fd = g_fd;
        pthread_mutex_unlock(&g_lock);
        if (!running || fd < 0) break;

        struct pollfd pfd = {.fd = fd, .events = POLLIN};
        int ready = poll(&pfd, 1, 200);
        if (ready <= 0) continue;
        if (!(pfd.revents & POLLIN)) continue;

        ssize_t n = read(fd, &ev, sizeof(ev));
        if (n < 0) {
            if (errno == EINTR || errno == EAGAIN) continue;
            LOGW("uhid read: %s", strerror(errno));
            break;
        }
        if (n != (ssize_t) sizeof(ev)) continue;

        switch (ev.type) {
            case UHID_START:
                LOGI("uhid START");
                break;
            case UHID_STOP:
                LOGI("uhid STOP");
                break;
            case UHID_OPEN:
                LOGI("uhid OPEN");
                break;
            case UHID_CLOSE:
                LOGI("uhid CLOSE");
                break;
            case UHID_GET_REPORT: {
                // We are a pure input device; answer so the kernel never blocks.
                memset(&reply, 0, sizeof(reply));
                reply.type = UHID_GET_REPORT_REPLY;
                reply.u.get_report_reply.id = ev.u.get_report.id;
                reply.u.get_report_reply.err = 1;
                pthread_mutex_lock(&g_lock);
                if (g_fd >= 0) write_event_locked(&reply);
                pthread_mutex_unlock(&g_lock);
                break;
            }
            case UHID_SET_REPORT:
                break;
            default:
                break;
        }
    }
    return NULL;
}

JNIEXPORT jint JNICALL
Java_org_decopad_pen_PenUserService_nativeCreate(JNIEnv *env, jclass clazz,
                                                 jbyteArray descriptor, jstring name,
                                                 jint vendor, jint product, jint version) {
    (void) clazz;
    if (descriptor == NULL) return -EINVAL;

    jsize length = (*env)->GetArrayLength(env, descriptor);
    if (length <= 0 || length > (jsize) sizeof(((struct uhid_create2_req *) 0)->rd_data)) {
        return -EINVAL;
    }

    pthread_mutex_lock(&g_lock);
    if (g_fd >= 0) {
        pthread_mutex_unlock(&g_lock);
        return -EBUSY;
    }

    int fd = open("/dev/uhid", O_RDWR | O_CLOEXEC);
    if (fd < 0) {
        int err = errno;
        pthread_mutex_unlock(&g_lock);
        LOGW("open /dev/uhid: %s", strerror(err));
        return -err;
    }

    struct uhid_event ev;
    memset(&ev, 0, sizeof(ev));
    ev.type = UHID_CREATE2;

    const char *cname = (*env)->GetStringUTFChars(env, name, NULL);
    strncpy((char *) ev.u.create2.name, cname != NULL ? cname : "decopad pen",
            sizeof(ev.u.create2.name) - 1);
    if (cname != NULL) (*env)->ReleaseStringUTFChars(env, name, cname);
    strncpy((char *) ev.u.create2.phys, "decopad", sizeof(ev.u.create2.phys) - 1);
    strncpy((char *) ev.u.create2.uniq, "decopad-uhid", sizeof(ev.u.create2.uniq) - 1);
    (*env)->GetByteArrayRegion(env, descriptor, 0, length,
                               (jbyte *) ev.u.create2.rd_data);
    ev.u.create2.rd_size = (__u16) length;
    ev.u.create2.bus = BUS_USB;
    ev.u.create2.vendor = (__u32) vendor;
    ev.u.create2.product = (__u32) product;
    ev.u.create2.version = (__u32) version;
    ev.u.create2.country = 0;

    g_fd = fd;
    int err = write_event_locked(&ev);
    if (err != 0) {
        g_fd = -1;
        close(fd);
        pthread_mutex_unlock(&g_lock);
        LOGW("UHID_CREATE2: %s", strerror(-err));
        return err;
    }

    g_reader_running = 1;
    if (pthread_create(&g_reader, NULL, reader_main, NULL) == 0) {
        g_reader_started = 1;
    } else {
        LOGW("reader thread failed");
    }
    pthread_mutex_unlock(&g_lock);

    LOGI("uhid device created (fd=%d, descriptor=%d bytes)", fd, (int) length);
    return fd;
}

JNIEXPORT jint JNICALL
Java_org_decopad_pen_PenUserService_nativeSubmit(JNIEnv *env, jclass clazz,
                                                 jbyteArray report) {
    (void) clazz;
    if (report == NULL) return -EINVAL;

    jsize length = (*env)->GetArrayLength(env, report);
    if (length <= 0 || length > (jsize) sizeof(((struct uhid_input2_req *) 0)->data)) {
        return -EINVAL;
    }

    struct uhid_event ev;
    memset(&ev, 0, sizeof(ev));
    ev.type = UHID_INPUT2;
    ev.u.input2.size = (__u16) length;
    (*env)->GetByteArrayRegion(env, report, 0, length, (jbyte *) ev.u.input2.data);

    pthread_mutex_lock(&g_lock);
    int err = g_fd >= 0 ? write_event_locked(&ev) : -EBADF;
    pthread_mutex_unlock(&g_lock);
    return err;
}

JNIEXPORT jboolean JNICALL
Java_org_decopad_pen_PenUserService_nativeIsOpen(JNIEnv *env, jclass clazz) {
    (void) env;
    (void) clazz;
    pthread_mutex_lock(&g_lock);
    jboolean open = g_fd >= 0 ? JNI_TRUE : JNI_FALSE;
    pthread_mutex_unlock(&g_lock);
    return open;
}

JNIEXPORT void JNICALL
Java_org_decopad_pen_PenUserService_nativeDestroy(JNIEnv *env, jclass clazz) {
    (void) env;
    (void) clazz;

    pthread_mutex_lock(&g_lock);
    int fd = g_fd;
    int started = g_reader_started;
    if (fd >= 0) {
        struct uhid_event ev;
        memset(&ev, 0, sizeof(ev));
        ev.type = UHID_DESTROY;
        write_event_locked(&ev);
    }
    g_reader_running = 0;
    g_fd = -1;
    g_reader_started = 0;
    pthread_mutex_unlock(&g_lock);

    if (started) pthread_join(g_reader, NULL);
    if (fd >= 0) close(fd);
    LOGI("uhid device destroyed");
}
