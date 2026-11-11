#include <Arduino.h>
#include <ctype.h>
#include <stdlib.h>
#include <string.h>

#ifdef ARDUINO_ARCH_ESP32
#include <esp_ota_ops.h>
#ifndef RAF_ADC_PIN
#define RAF_ADC_PIN 34
#endif
#define RAF_BOARD_NAME "ESP32"
#else
#ifndef RAF_ADC_PIN
#define RAF_ADC_PIN A0
#endif
#define RAF_BOARD_NAME "Arduino"
#endif

#if !defined(RAF_LED_PIN) && defined(LED_BUILTIN)
#define RAF_LED_PIN LED_BUILTIN
#endif

static char frame[192];
static size_t frame_length = 0;
static bool oversized = false;
static unsigned long last_sample = 0;

static const char* field(const char* key) {
  const char* value = strstr(frame, key);
  if (!value) return nullptr;
  value = strchr(value, ':');
  if (!value) return nullptr;
  do { ++value; } while (isspace(static_cast<unsigned char>(*value)));
  return value;
}

static void hello() {
  Serial.print(F("{\"type\":\"hello\",\"protocol\":1,\"board\":\"" RAF_BOARD_NAME "\",\"firmware\":\"raf-lab-1\""));
#ifdef RAF_LED_PIN
  Serial.print(F(",\"outputs\":[{\"id\":\"led\",\"min\":0,\"max\":1}]"));
#endif
#ifdef ARDUINO_ARCH_ESP32
  const esp_partition_t* next = esp_ota_get_next_update_partition(nullptr);
  if (next && next->size > 0) {
    Serial.print(F(",\"ota\":{\"target\":\"" CONFIG_IDF_TARGET "\",\"max_image_bytes\":"));
    Serial.print(static_cast<unsigned long>(next->size));
    Serial.print('}');
  }
#endif
  Serial.println('}');
}

static void command() {
  const char* type = field("\"type\"");
  if (!type) return;
  if (strncmp(type, "\"identify\"", 10) == 0) {
    hello();
    return;
  }
  if (strncmp(type, "\"set\"", 5) != 0) return;
  const char* id_field = field("\"id\"");
  if (!id_field || !isdigit(static_cast<unsigned char>(*id_field))) return;
  const unsigned long id = strtoul(id_field, nullptr, 10);
  bool accepted = false;
#ifdef RAF_LED_PIN
  const char* output = field("\"output\"");
  const char* value = field("\"value\"");
  if (output && strncmp(output, "\"led\"", 5) == 0 && value) {
    char* end = nullptr;
    const double level = strtod(value, &end);
    if (end != value && (level == 0 || level == 1)) {
      digitalWrite(RAF_LED_PIN, level == 1 ? HIGH : LOW);
      accepted = true;
    }
  }
#endif
  Serial.print(F("{\"type\":\"ack\",\"id\":"));
  Serial.print(id);
  Serial.println(accepted ? F(",\"ok\":true}") : F(",\"ok\":false}"));
}

void setup() {
#ifdef RAF_LED_PIN
  pinMode(RAF_LED_PIN, OUTPUT);
  digitalWrite(RAF_LED_PIN, LOW);
#endif
  Serial.begin(115200);
}

void loop() {
  for (uint8_t budget = 64; budget > 0 && Serial.available(); --budget) {
    const char byte = static_cast<char>(Serial.read());
    if (byte == '\n') {
      if (!oversized && frame_length > 0) {
        frame[frame_length] = '\0';
        command();
      }
      oversized = false;
      frame_length = 0;
    } else if (frame_length == sizeof(frame) - 1) {
      oversized = true;
    } else if (!oversized) {
      frame[frame_length++] = byte;
    }
  }
  const unsigned long now = millis();
  if (now - last_sample >= 100) {
    last_sample = now;
    Serial.print(F("{\"type\":\"sample\",\"channel\":\"analog\",\"value\":"));
    Serial.print(analogRead(RAF_ADC_PIN));
    Serial.println(F(",\"unit\":\"count\"}"));
  }
}
