package c8y.lwm2m.weptech.action;

import com.cumulocity.microservice.customdecoders.api.exception.DecoderServiceException;
import com.cumulocity.microservice.customdecoders.api.model.DecoderResult;
import com.cumulocity.microservice.customdecoders.api.service.DecoderService;
import com.cumulocity.microservice.customdecoders.api.util.DecoderUtils;
import com.cumulocity.model.idtype.GId;
import com.cumulocity.model.measurement.MeasurementValue;
import com.cumulocity.rest.representation.measurement.MeasurementCollectionRepresentation;
import com.cumulocity.rest.representation.measurement.MeasurementRepresentation;
import com.cumulocity.sdk.client.inventory.InventoryApi;
import com.cumulocity.sdk.client.measurement.MeasurementApi;
import lombok.extern.slf4j.Slf4j;
import org.joda.time.DateTime;

import com.cumulocity.microservice.context.ContextService;
import com.cumulocity.microservice.context.credentials.MicroserviceCredentials;
import com.cumulocity.sdk.client.RestConnector;


import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.stereotype.Service;


import java.math.BigDecimal;
import java.util.*;
import java.io.IOException;
import java.net.URI;
import java.net.http.HttpClient;
import java.net.http.HttpRequest;
import java.net.http.HttpResponse;
import java.nio.charset.StandardCharsets;

import org.json.JSONArray;
import org.json.JSONObject;

import org.apache.commons.csv.CSVFormat;
import org.apache.commons.csv.CSVParser;
import org.apache.commons.csv.CSVRecord;

import java.io.StringReader;

import com.cumulocity.model.ID;
import com.cumulocity.rest.representation.identity.ExternalIDRepresentation;
import com.cumulocity.sdk.client.identity.IdentityApi;
import  com.cumulocity.rest.representation.inventory.ManagedObjectRepresentation;

import com.fasterxml.jackson.databind.JsonNode;
import com.fasterxml.jackson.databind.ObjectMapper;
import com.fasterxml.jackson.databind.node.ObjectNode;

//@Component
@Slf4j
@Service
public class OMSDecoderC8YParserService implements DecoderService{

    @Autowired
    IdentityApi identityApi;

    @Autowired
    InventoryApi inventoryApi;

    @Autowired
    MeasurementApi measurementApi;

    private final RestConnector restConnector;
    private final ContextService<MicroserviceCredentials> contextService;
    private final ObjectMapper objectMapper;

    // Use the explicit internal reverse-proxy path target registered via your cumulocity.json manifest
    private static final String PARSER_INTERNAL_PATH = "/service/c8y-oms-parser/api/v1/parse";

    @Autowired
    public OMSDecoderC8YParserService(RestConnector restConnector,
                                      ContextService<MicroserviceCredentials> contextService) {
        this.restConnector = restConnector;
        this.contextService = contextService;
        this.objectMapper = new ObjectMapper();
    }

    @Override
    public DecoderResult decode(String payloadToDecode, GId sourceDeviceId, Map<String, String> inputArguments) throws DecoderServiceException {
        DecoderResult decoderResult = new DecoderResult();
        String decodedString = "";
        String meterSerial = "";
        String meterPayload = "";
        String meterEncryptionkey = "";
        final String EXTERNAL_ID_TYPE = "c8y_Serial";
        //final String ENCRYPTION_KEY_TYPE = "axioma_encryption_key";

        log.info("Decoding OMS payload HEX {}.", payloadToDecode);

       byte[] decodedBytes = DecoderUtils.hexStringToByteArray(payloadToDecode);

       try {
           decodedString = new String(decodedBytes, "US-ASCII");

           log.info("Decoding OMS payload String {}.", decodedString);

           //extract meter data
           CSVParser parser = CSVFormat.DEFAULT.parse(new StringReader(decodedString));
           CSVRecord record = parser.getRecords().get(0);

           if (record == null || record.size() < 8) {
               log.error("Extracted Meter Payload from Weptech GW is less than 8 elements.");
               return decoderResult;
               //throw new DecoderServiceException(new Throwable("Extracted Meter Payload from Weptech GW is less than 8 elements."), "Extracted Meter Payload from Weptech GW is less than 8 elements: " + record ,decoderResult );
           }

           meterSerial = record.get(7);
           meterPayload = record.get(8);

           log.info("Meter Serial ID {}, Meter Payload {}", meterSerial, meterPayload);

           // Get Meter's Managed Object ID
           ManagedObjectRepresentation  meterMO = getDeviceByExternalId(EXTERNAL_ID_TYPE, meterSerial);

           if (meterMO != null) {

               // call C*Y OMS parser
               String responseBody = parseOMSPayload(meterPayload);

               if (responseBody == null) {
                   log.error("There is no Response from OMS Parser ");
                   throw new DecoderServiceException(new Throwable("There is no Response from OMS Parser"), "There is no Response from OMS Parser",decoderResult );
               }

               // parse response body
               parsewMbusPayload(meterMO, responseBody);

           }

           log.info("Meter MO {}", meterMO);


       } catch (java.io.UnsupportedEncodingException e) {
           throw new DecoderServiceException(new Throwable("Unsupported Encoding"), "Unsupported Encoding US-ASCII", decoderResult);
       }  catch (java.io.IOException e) {
           throw new DecoderServiceException(new Throwable("IO Exception"), "IO Exception from parsing Weptech payload", decoderResult);
       }


        log.info("Finished decoding OMS Payload");
        return decoderResult;

    }

    private  String parseOMSPayload(String payloadToDecode) {

        MicroserviceCredentials credentials = contextService.getContext();
        if (credentials == null) {
            throw new IllegalStateException("Execution thread must be running within an active Cumulocity tenant context.");
        }

        try {
            ObjectNode requestBody = objectMapper.createObjectNode();
            requestBody.put("payload", payloadToDecode);
            String dynamicJsonBody = objectMapper.writeValueAsString(requestBody);

            log.info("Invoking custom c8y-oms-parser via native Java HttpClient...");
            log.info("Sending payload to parser microservice...");

            // 1. Manually build the standard Cumulocity Basic Auth token: tenant/username:password
            String baseUrl = System.getenv("C8Y_BASEURL");
            String authString = credentials.getTenant() + "/" + credentials.getUsername() + ":" + credentials.getPassword();
            String encodedAuth = Base64.getEncoder().encodeToString(authString.getBytes(StandardCharsets.UTF_8));
            String basicAuthHeader = "Basic " + encodedAuth;
            //String constantJsonBody = "{\"payload\": \"YUQJB3RFcgkgB3pKEAAABG0NCl02BCCgNUcBBBMAAAAABJM7AAAAAASTPAAAAAACOwAAAlnw2ERtAABBNkQTAAAAAESTOwAAAABEkzwAAAAANP0XAQAAAAQkVzNHAQH9dGE=\"}";

            HttpClient client = HttpClient.newBuilder()
                    .version(HttpClient.Version.HTTP_1_1)
                    .build();

            log.info("Base URL is ===> : " + baseUrl);

            // 2. Build the request forwarding the token to the local sidecar proxy
            HttpRequest request = HttpRequest.newBuilder()
                    //.uri(URI.create("http://localhost/service/c8y-oms-parser/api/v1/decode"))
                    //.uri(URI.create(baseUrl + "/service/c8y-oms-parser/api/v1/decode"))
                    .uri(URI.create(baseUrl + PARSER_INTERNAL_PATH))
                    //.uri(URI.create("http://c8y-oms-parser/api/v1/decode"))
                    .header("Content-Type", "application/json")
                    .header("Accept", "application/json")
                    .header("Authorization", basicAuthHeader)
                    .header("X-Cumulocity-Application-Key", "c8y-oms-parser")
                    //.POST(HttpRequest.BodyPublishers.ofString(requestBody.toString()))
                    .POST(HttpRequest.BodyPublishers.ofString(dynamicJsonBody))
                    .build();

            HttpResponse<String> response = client.send(request, HttpResponse.BodyHandlers.ofString());

            if (response.statusCode() >= 200 && response.statusCode() < 300) {
                log.info("SUCCESS! Constant payload communication works. Response: {}", response.body());
                return response.body();
            } else {
                // FORCEFUL EXCEPTION: This breaks the execution and prints the exact error response body to your terminal logs
                throw new DecoderServiceException(new Throwable("HTTP FAILURE CODE"), "HTTP FAILURE CODE: " + response.statusCode() + " | PROXY ERROR BODY: " + response.body(), new DecoderResult());
            }
        } catch (Exception e) {
            log.error("CRITICAL EXCEPTION PASSED INSIDE COMMUNICATION LAYER: ", e);
            return null;
            //throw new DecoderServiceException(new Throwable("Underlying communication failure"), "Underlying communication failure", new DecoderResult());
        }
    }

    /*private  JSONObject extractTemperatureData(JSONObject payload) {
        JSONObject apl = payload.getJSONObject("APL");
        JSONObject deviceIdObj = apl.getJSONObject("DeviceId");

        // Extract DeviceId and DeviceType
        String deviceId = deviceIdObj.getString("DinAddress");
        String deviceType = deviceIdObj.getString("DeviceTypeByName");

        // Extract Temperature value and unit
        JSONArray drArray = apl.getJSONArray("DR");
        Float tempValue = null;
        String tempUnit = null;

        for (int i = 0; i < drArray.length(); i++) {
            JSONObject drItem = drArray.getJSONObject(i);
            if ("Instantaneous value".equals(drItem.getString("FunctionByName")) &&
                    "External temperature".equals(drItem.getString("Description"))) {
                tempValue = Float.parseFloat(drItem.getString("Value"));
                tempUnit = drItem.getString("Unit");
                break;
            }
        }

        // Build result JSON
        JSONObject result = new JSONObject();
        result.put("DeviceId", deviceId);
        result.put("DeviceType", deviceType);
        result.put("TemperatureValue", tempValue);
        result.put("TemperatureUnit", tempUnit);

        return result;
    }*/


    public ManagedObjectRepresentation getDeviceByExternalId(String type, String externalId) throws DecoderServiceException {
        ID extId = new ID(type, externalId);
        ManagedObjectRepresentation meterMO = null;
        GId ManagedObjectID = null;

        try {
            // 3. Query the Identity API
            ExternalIDRepresentation representation = identityApi.getExternalId(extId);

            // 4. Extract the internal Global ID (Device ID)
            ManagedObjectID = representation.getManagedObject().getId();

            // get Managed Object
            meterMO = inventoryApi.get(ManagedObjectID);


            log.info("Meter MO {}", meterMO);
        } catch (com.cumulocity.sdk.client.SDKException e) {
            if (e.getHttpStatus() == 404) {
                log.error("Meter is  not found the given External ID {}.", externalId);
                throw new DecoderServiceException(new Throwable("Meter is not found"), "Meter is not found", new DecoderResult());
            } else {
                throw new DecoderServiceException(e, e.getMessage(), new DecoderResult());
            }
        }
        return meterMO;
    }

    public void parsewMbusPayload(ManagedObjectRepresentation meterMo, String jsonString) throws DecoderServiceException {
        try {
            ObjectMapper mapper = new ObjectMapper();
            JsonNode root = mapper.readTree(jsonString);
            JsonNode drArray = root.at("/ParsedMeasurements");

            MeasurementRepresentation volumeMeasurement = new MeasurementRepresentation();
            MeasurementRepresentation volumeFlowMeasurement = new MeasurementRepresentation();
            MeasurementRepresentation flowTemperatureMeasurement = new MeasurementRepresentation();
            MeasurementRepresentation remainingBatteryMeasurement = new MeasurementRepresentation();

            //Set source ID
            volumeMeasurement.setSource(meterMo);
            volumeFlowMeasurement.setSource(meterMo);
            flowTemperatureMeasurement.setSource(meterMo);
            remainingBatteryMeasurement.setSource(meterMo);

            if (drArray.isArray()) {
                for (JsonNode node : drArray) {
                    String header = node.get("HeaderRaw").asText();
                    String value = node.has("Value") ? node.get("Value").asText() : "";
                    String unit = node.has("Unit") ? node.get("Unit").asText() : "";

                    switch (header) {
                        case "046D": // 1. Date and Time
                            DateTime measurementTime = DateTime.now();

                            volumeMeasurement.setDateTime(measurementTime);
                            volumeFlowMeasurement.setDateTime(measurementTime);
                            flowTemperatureMeasurement.setDateTime(measurementTime);
                            remainingBatteryMeasurement.setDateTime(measurementTime);

                            break;

                        case "0413": // 2. Volume
                            log.info("Volume: {} {}", value, unit);

                            volumeMeasurement.setType("C8y_Meter_Volume");

                            MeasurementValue volumeValue = new MeasurementValue();
                            volumeValue.setValue(new BigDecimal(value));
                            volumeValue.setUnit(unit);

                            Map<String, MeasurementValue> volumeSeries = new HashMap<>();
                            volumeSeries.put("V", volumeValue);

                            volumeMeasurement.set(volumeSeries, "Meter_Volume");

                            break;

                        case "023B": // 3. Volume Flow
                            log.info("Volume Flow: {} {}", value, unit);

                            volumeFlowMeasurement.setType("C8y_Meter_Volume_Flow");
                            MeasurementValue volumeFlowValue = new MeasurementValue();
                            volumeFlowValue.setValue(new BigDecimal(value));
                            volumeFlowValue.setUnit(unit);

                            Map<String, MeasurementValue> volumeFlowSeries = new HashMap<>();
                            volumeFlowSeries.put("Flow", volumeFlowValue);

                            volumeFlowMeasurement.set(volumeFlowSeries, "Meter_Volume_Flow");
                            break;

                        case "0259": // 4. Flow Temperature
                            log.info("Flow Temperature: {} {}", value, unit);

                            flowTemperatureMeasurement.setType("C8y_Meter_Flow_Temperature");
                            MeasurementValue flowTemperatureValue = new MeasurementValue();
                            flowTemperatureValue.setValue(new BigDecimal(value));
                            flowTemperatureValue.setUnit(unit);

                            Map<String, MeasurementValue> flowTemperatureSeries = new HashMap<>();
                            flowTemperatureSeries.put("T", flowTemperatureValue);

                            flowTemperatureMeasurement.set(flowTemperatureSeries, "Meter_Flow_Temperature");
                            break;

                        case "01FD74": // 5. Remaining Battery
                            log.info("Remaining Battery: {} {}", value, unit);

                            remainingBatteryMeasurement.setType("C8y_Meter_Remaining_Battery");
                            MeasurementValue remainingBatteryValue = new MeasurementValue();
                            remainingBatteryValue.setValue(new BigDecimal(value));
                            remainingBatteryValue.setUnit(unit);

                            Map<String, MeasurementValue> remainingBatterySeries = new HashMap<>();
                            remainingBatterySeries.put("Remaining_Battery", remainingBatteryValue);

                            remainingBatteryMeasurement.set(remainingBatterySeries, "Meter_Remaining_Battery");
                            break;
                    }
                } // end of for

                // create Measurments
                List<MeasurementRepresentation> list = Arrays.asList(
                        volumeMeasurement,
                        volumeFlowMeasurement,
                        flowTemperatureMeasurement,
                        remainingBatteryMeasurement
                );
                MeasurementCollectionRepresentation collection = new MeasurementCollectionRepresentation();
                collection.setMeasurements(list);
                measurementApi.createBulk(collection);
                log.info("Measurements are created successfully .....");
            }
        } catch (Exception e) {
            log.error("Cannot Parse wMbus Payload.");
            throw new DecoderServiceException(e, e.getMessage(), new DecoderResult());

        }
    }

}
