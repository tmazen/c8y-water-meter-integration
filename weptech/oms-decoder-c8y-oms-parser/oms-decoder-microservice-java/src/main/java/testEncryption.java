import org.apache.commons.csv.CSVFormat;
import org.apache.commons.csv.CSVParser;
import org.apache.commons.csv.CSVRecord;

import javax.crypto.Cipher;
import javax.crypto.spec.IvParameterSpec;
import javax.crypto.spec.SecretKeySpec;
import java.io.StringReader;
import java.util.Arrays;
import java.util.Base64;

public class testEncryption {

    public static byte[] hexStringToByteArray(String s) {
        int len = s.length();
        byte[] data = new byte[len / 2];
        for (int i = 0; i < len; i += 2) {
            data[i / 2] = (byte) ((Character.digit(s.charAt(i), 16) << 4)
                    + Character.digit(s.charAt(i + 1), 16));
        }
        return data;
    }

    public static String bytesToHex(byte[] bytes) {
        StringBuilder sb = new StringBuilder();
        for (byte b : bytes) {
            sb.append(String.format("%02X", b));
        }
        return sb.toString();
    }

    public static byte[] reverseBytes(byte[] in) {
        byte[] out = new byte[in.length];
        for (int i = 0; i < in.length; i++) {
            out[i] = in[in.length - 1 - i];
        }
        return out;
    }

    /**
     * Decrypts AES CBC ciphertext using the specified key, IV, and ciphertext offset.
     */
    public static byte[] decryptRaw(byte[] rawFrame, byte[] aesKey, byte[] iv, int ciphertextOffset) throws Exception {
        int rawCiphertextLen = rawFrame.length - ciphertextOffset;
        int validCiphertextLen = (rawCiphertextLen / 16) * 16;

        if (validCiphertextLen <= 0) {
            return new byte[0];
        }

        byte[] ciphertext = new byte[validCiphertextLen];
        System.arraycopy(rawFrame, ciphertextOffset, ciphertext, 0, validCiphertextLen);

        Cipher cipher = Cipher.getInstance("AES/CBC/NoPadding");
        SecretKeySpec keySpec = new SecretKeySpec(aesKey, "AES");
        IvParameterSpec ivSpec = new IvParameterSpec(iv);
        cipher.init(Cipher.DECRYPT_MODE, keySpec, ivSpec);

        return cipher.doFinal(ciphertext);
    }

    /**
     * Standard wM-Bus OMS Mode 5 IV Construction
     */
    public static byte[] buildStandardIV(byte[] rawFrame, int accessNoIndex) {
        byte[] iv = new byte[16];
        // 8 Bytes Header: M_ID(2) + Addr(4) + Version(1) + Type(1)
        System.arraycopy(rawFrame, 2, iv, 0, 8);

        // 8 Bytes Access Number repeated
        byte accessNo = (rawFrame.length > accessNoIndex) ? rawFrame[accessNoIndex] : 0x00;
        Arrays.fill(iv, 8, 16, accessNo);
        return iv;
    }

    /**
     * Reconstructs a valid unencrypted wM-Bus frame (CI = 0x78) without filler padding.
     */
    public static byte[] prepareDiehlHydrusFrame(byte[] rawFrame, byte[] decryptedBytes) {
        if (decryptedBytes.length < 2 || decryptedBytes[0] != (byte) 0x2F || decryptedBytes[1] != (byte) 0x2F) {
            throw new IllegalArgumentException("Decryption failed! Payload does not start with 0x2F 0x2F OMS verification bytes.");
        }

        // 1. Skip OMS 2-byte verification header (0x2F 0x2F)
        int offset = 2;
        int cleanDataLen = decryptedBytes.length - offset;

        // 2. Trim trailing AES block padding (0x2F bytes at the end)
        while (cleanDataLen > 0 && decryptedBytes[offset + cleanDataLen - 1] == (byte) 0x2F) {
            cleanDataLen--;
        }

        // 3. Build wM-Bus frame: 10-byte Link Header + CI (0x78) + Plaintext Records
        byte[] fullFrame = new byte[11 + cleanDataLen];
        System.arraycopy(rawFrame, 0, fullFrame, 0, 10);
        fullFrame[10] = (byte) 0x78; // Set CI byte to 0x78 Plaintext Short Header
        System.arraycopy(decryptedBytes, offset, fullFrame, 11, cleanDataLen);

        // 4. Update total length byte L (Index 0)
        fullFrame[0] = (byte) (fullFrame.length - 1);

        return fullFrame;
    }

    public static void main(String[] args) throws Exception {
        String base64Payload = "bkQJB3RFcgkgB3p6EGAFOV5D846l2scdsSFAc8TQqDh7nvv+lMLArxbnBqA1YegyfRazyZ9ocF7KYro+62Oqc95Jscd53MitowL1Af3fIvr+BdsOIQM5jrb2VHIU2D5oLW6qINCljG4YaQIalNM5";
        String keyHex = "4207A729560A4258530A331FD66078BE";


        byte[] rawFrame = Base64.getDecoder().decode(base64Payload);
        byte[] key = hexStringToByteArray(keyHex);

        System.out.println("--- Starting Decryption Diagnostics ---");
        System.out.println("Raw Frame Byte Count: " + rawFrame.length);
        System.out.println("CI Byte at index 10: 0x" + String.format("%02X", rawFrame[10]));

        boolean success = false;
        byte[][] keysToTest = { key, reverseBytes(key) };
        String[] keyNames = { "Original Key", "Reversed Endian Key" };

        for (int k = 0; k < keysToTest.length; k++) {
            byte[] currentKey = keysToTest[k];

            // Test standard ciphertext starting offsets (10 to 25)
            for (int offset = 10; offset <= 25; offset++) {
                // Test access number position (indices 11 to 14)
                for (int accIdx = 11; accIdx <= 14; accIdx++) {
                    byte[] iv = buildStandardIV(rawFrame, accIdx);
                    byte[] decrypted = decryptRaw(rawFrame, currentKey, iv, offset);

                    if (decrypted.length >= 2 && decrypted[0] == (byte) 0x2F && decrypted[1] == (byte) 0x2F) {
                        System.out.println("\nSUCCESS! Valid decryption found!");
                        System.out.println("Key Variant: " + keyNames[k]);
                        System.out.println("Ciphertext Offset: " + offset);
                        System.out.println("Access Number Index in IV: " + accIdx + " (Value: 0x" + String.format("%02X", rawFrame[accIdx]) + ")");
                        System.out.println("Decrypted Payload Hex:\n" + bytesToHex(decrypted));

                        byte[] reconstructed = prepareDiehlHydrusFrame(rawFrame, decrypted);
                        String base64ForRust = Base64.getEncoder().encodeToString(reconstructed);

                        System.out.println("\n--- Reconstructed Frame Output ---");
                        System.out.println("Reconstructed Frame Hex:\n" + bytesToHex(reconstructed));
                        System.out.println("\nBase64 Payload to send to Rust API:");
                        System.out.println(base64ForRust);

                        success = true;
                        break;
                    }
                }
                if (success) break;
            }
            if (success) break;
        }

        if (!success) {
            System.err.println("\nCRITICAL: No offset or key variant produced a payload starting with '2F 2F'.");
            System.err.println("This indicates that the AES Key '" + keyHex + "' does not match Meter Address " + bytesToHex(Arrays.copyOfRange(rawFrame, 4, 8)));
        }
    }
}