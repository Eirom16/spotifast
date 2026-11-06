package dev.eirom.doremi;

import android.content.Context;
import android.net.Uri;
import android.security.keystore.KeyGenParameterSpec;
import android.security.keystore.KeyProperties;
import android.util.AtomicFile;
import java.io.ByteArrayOutputStream;
import java.io.File;
import java.io.FileInputStream;
import java.io.FileOutputStream;
import java.io.IOException;
import java.io.InputStream;
import java.nio.charset.StandardCharsets;
import java.security.GeneralSecurityException;
import java.security.KeyStore;
import java.security.MessageDigest;
import java.util.Arrays;
import javax.crypto.Cipher;
import javax.crypto.KeyGenerator;
import javax.crypto.SecretKey;
import javax.crypto.spec.GCMParameterSpec;

/** Worker-only platform storage. Account validation remains in shared Rust. */
public final class YoutubeSessionStore {
    private YoutubeSessionStore() {}
    private static final int LIMIT=64*1024;
    private static final byte[] MAGIC={'D','Y','M',1};
    private static final String FILE="youtube-session-v1.bin";
    // A stalled document provider cannot retain the Rust actor indefinitely or
    // create an unbounded set of IO threads. No queue and at most one reader.
    private static final java.util.concurrent.ThreadPoolExecutor DOCUMENTS=new java.util.concurrent.ThreadPoolExecutor(
        0,1,30,java.util.concurrent.TimeUnit.SECONDS,new java.util.concurrent.SynchronousQueue<>(),task -> {
            Thread thread=new Thread(task,"doremi-session-document");thread.setDaemon(true);return thread;
        },new java.util.concurrent.ThreadPoolExecutor.AbortPolicy());
    private static File directory(Context context,String value) throws IOException {
        File root=context.getFilesDir().getCanonicalFile();
        File directory=new File(value).getCanonicalFile();
        if(!directory.equals(root) && !directory.getPath().startsWith(root.getPath()+File.separator))
            throw new IOException("Invalid session directory");
        return directory;
    }
    private static String alias(File directory) throws GeneralSecurityException {
        byte[] hash=MessageDigest.getInstance("SHA-256").digest(directory.getPath().getBytes(StandardCharsets.UTF_8));
        StringBuilder value=new StringBuilder("dev.eirom.doremi.youtube.v1.");
        for(byte b:hash) value.append(String.format(java.util.Locale.ROOT,"%02x",b&255));
        return value.toString();
    }
    private static SecretKey key(File directory,boolean create) throws GeneralSecurityException,IOException {
        KeyStore store=KeyStore.getInstance("AndroidKeyStore");store.load(null);
        String alias=alias(directory);
        if(store.containsAlias(alias)) return (SecretKey)store.getKey(alias,null);
        if(!create) throw new IOException("Session key unavailable");
        KeyGenerator generator=KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES,"AndroidKeyStore");
        generator.init(new KeyGenParameterSpec.Builder(alias,KeyProperties.PURPOSE_ENCRYPT|KeyProperties.PURPOSE_DECRYPT)
            .setKeySize(256).setBlockModes(KeyProperties.BLOCK_MODE_GCM)
            .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
            .setRandomizedEncryptionRequired(true).build());
        return generator.generateKey();
    }
    private static byte[] read(InputStream input,int limit) throws IOException {
        long deadline=android.os.SystemClock.elapsedRealtime()+8000;
        ByteArrayOutputStream bytes=new ByteArrayOutputStream();byte[] buffer=new byte[4096];
        for(int count;(count=input.read(buffer))!=-1;) {
            if(android.os.SystemClock.elapsedRealtime()>deadline) throw new java.io.InterruptedIOException("Session input deadline");
            if(count>limit-bytes.size()) throw new IllegalArgumentException("Session input too large");
            bytes.write(buffer,0,count);
        }
        return bytes.toByteArray();
    }
    public static byte[] readDocument(Context context,String value) throws IOException {
        Uri uri=Uri.parse(value);
        if(!"content".equals(uri.getScheme())) throw new IOException("Unsupported session document");
        java.util.concurrent.Future<byte[]> task;
        try {
            task=DOCUMENTS.submit(() -> {
                try(InputStream input=context.getContentResolver().openInputStream(uri)) {
                    if(input==null) throw new IOException("Session document unavailable");
                    return read(input,LIMIT);
                }
            });
        } catch(java.util.concurrent.RejectedExecutionException busy) {throw new IOException("Session document reader busy");}
        try {return task.get(8,java.util.concurrent.TimeUnit.SECONDS);}
        catch(java.util.concurrent.TimeoutException timeout) {task.cancel(true);throw new java.io.InterruptedIOException("Session input deadline");}
        catch(InterruptedException cancelled) {task.cancel(true);Thread.currentThread().interrupt();throw new java.io.InterruptedIOException("Session input cancelled");}
        catch(java.util.concurrent.ExecutionException failed) {
            if(failed.getCause() instanceof IllegalArgumentException) throw new IllegalArgumentException("Session input too large");
            throw new IOException("Session document unavailable");
        }
    }
    public static synchronized byte[] load(Context context,String path) throws IOException {
        File directory=directory(context,path);AtomicFile file=new AtomicFile(new File(directory,FILE));
        byte[] bytes;
        try(FileInputStream input=file.openRead()) {bytes=read(input,LIMIT+32);}
        catch(java.io.FileNotFoundException absent) {return null;}
        if(bytes.length<32 || !Arrays.equals(Arrays.copyOf(bytes,4),MAGIC))
            throw new IOException("Unsupported session storage");
        try {
            Cipher cipher=Cipher.getInstance("AES/GCM/NoPadding");
            cipher.init(Cipher.DECRYPT_MODE,key(directory,false),new GCMParameterSpec(128,bytes,4,12));
            cipher.updateAAD(MAGIC);
            return cipher.doFinal(bytes,16,bytes.length-16);
        } catch(GeneralSecurityException error) {throw new IOException("Session storage could not be opened");}
    }
    public static synchronized void store(Context context,String path,byte[] secret) throws IOException {
        if(secret.length==0 || secret.length>LIMIT) throw new IOException("Invalid session size");
        File directory=directory(context,path);
        if(!directory.isDirectory() && !directory.mkdirs()) throw new IOException("Session storage unavailable");
        AtomicFile file=new AtomicFile(new File(directory,FILE));
        // Preserve future envelopes rather than silently replacing their data.
        try(FileInputStream existing=file.openRead()) {
            byte[] magic=new byte[4];int offset=0;
            while(offset<4) {int count=existing.read(magic,offset,4-offset);if(count<0) break;offset+=count;}
            if(offset==4 && !Arrays.equals(magic,MAGIC)) throw new IOException("Unsupported session storage");
        } catch(java.io.FileNotFoundException absent) { /* New explicit connection. */ }
        byte[] sealed;
        try {
            Cipher cipher=Cipher.getInstance("AES/GCM/NoPadding");cipher.init(Cipher.ENCRYPT_MODE,key(directory,true));
            cipher.updateAAD(MAGIC);byte[] iv=cipher.getIV();
            if(iv.length!=12) throw new IOException("Unsupported session nonce");
            byte[] ciphertext=cipher.doFinal(secret);sealed=new byte[16+ciphertext.length];
            System.arraycopy(MAGIC,0,sealed,0,4);System.arraycopy(iv,0,sealed,4,12);
            System.arraycopy(ciphertext,0,sealed,16,ciphertext.length);
        } catch(GeneralSecurityException error) {throw new IOException("Session storage could not be protected");}
        FileOutputStream output=null;
        try {output=file.startWrite();output.write(sealed);file.finishWrite(output);}
        catch(IOException error) {if(output!=null) file.failWrite(output);throw new IOException("Session storage write failed");}
    }
    public static synchronized void forget(Context context,String path) throws IOException {
        File directory=directory(context,path);
        try {
            KeyStore store=KeyStore.getInstance("AndroidKeyStore");store.load(null);store.deleteEntry(alias(directory));
        } catch(GeneralSecurityException error) {throw new IOException("Session key could not be removed");}
        AtomicFile file=new AtomicFile(new File(directory,FILE));file.delete();
        if(file.getBaseFile().exists()) throw new IOException("Session storage could not be removed");
    }
}
